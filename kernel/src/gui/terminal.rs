//! Simple GUI terminal for OxideOS.
//!
//! This is an in-kernel command terminal intended for debugging and early
//! system bring-up. It is not a process-backed shell yet.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;

use crate::kernel::{keyboard, syscall, timer};

use super::colors;
use super::fonts;
use super::graphics::Graphics;
use super::window_manager::WindowManager;

const HISTORY_LIMIT: usize = 160;
const INPUT_QUEUE_SIZE: usize = 256;
const CHAR_WIDTH: u64 = 9;
const LINE_HEIGHT: u64 = 16;
const CONTENT_PADDING_X: u64 = 10;
const CONTENT_PADDING_Y: u64 = 8;
const INPUT_HEIGHT: u64 = 24;

static mut INPUT_QUEUE: [u8; INPUT_QUEUE_SIZE] = [0; INPUT_QUEUE_SIZE];
static mut INPUT_HEAD: usize = 0;
static mut INPUT_TAIL: usize = 0;

fn with_interrupts_disabled<T>(f: impl FnOnce() -> T) -> T {
    let flags: u64;
    unsafe {
        asm!("pushfq; pop {}", out(reg) flags, options(nomem, preserves_flags));
        asm!("cli", options(nomem, preserves_flags));
    }

    let result = f();

    if (flags & (1 << 9)) != 0 {
        unsafe {
            asm!("sti", options(nomem, preserves_flags));
        }
    }

    result
}

unsafe fn queue_key(ch: u8) {
    let next_tail = (INPUT_TAIL + 1) % INPUT_QUEUE_SIZE;
    if next_tail != INPUT_HEAD {
        INPUT_QUEUE[INPUT_TAIL] = ch;
        INPUT_TAIL = next_tail;
    }
}

fn dequeue_key() -> Option<u8> {
    with_interrupts_disabled(|| unsafe {
        if INPUT_HEAD == INPUT_TAIL {
            None
        } else {
            let ch = INPUT_QUEUE[INPUT_HEAD];
            INPUT_HEAD = (INPUT_HEAD + 1) % INPUT_QUEUE_SIZE;
            Some(ch)
        }
    })
}

unsafe fn terminal_key_callback(ch: u8) {
    queue_key(ch);
}

pub unsafe fn install_input_hooks() {
    keyboard::register_key_callback(terminal_key_callback);
}

pub struct TerminalApp {
    window_id: usize,
    history: Vec<String>,
    input: String,
}

impl TerminalApp {
    pub fn new(window_id: usize) -> Self {
        let mut terminal = Self {
            window_id,
            history: Vec::new(),
            input: String::new(),
        };
        terminal.print_banner();
        terminal
    }

    pub fn window_id(&self) -> usize {
        self.window_id
    }

    pub fn process_pending_input(&mut self, focused: bool) -> bool {
        let mut changed = false;

        while let Some(ch) = dequeue_key() {
            if focused {
                changed |= self.handle_key(ch);
            }
        }

        changed
    }

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager) {
        if !wm.is_window_visible(self.window_id) {
            return;
        }

        let Some(window) = wm.get_window(self.window_id) else {
            return;
        };

        let is_focused = wm.get_focused() == Some(self.window_id);
        let content_x = window.x + CONTENT_PADDING_X;
        let content_y = window.y + 30 + CONTENT_PADDING_Y;
        let content_width = window.width.saturating_sub(CONTENT_PADDING_X * 2);
        let content_height = window.height.saturating_sub(30 + CONTENT_PADDING_Y * 2);
        let output_height = content_height.saturating_sub(INPUT_HEIGHT + 6);
        let input_y = content_y + output_height + 6;
        let max_chars = (content_width / CHAR_WIDTH).max(4) as usize;
        let visible_lines = (output_height / LINE_HEIGHT).max(1) as usize;

        graphics.fill_rect(
            content_x - 2,
            content_y - 2,
            content_width + 4,
            content_height + 4,
            colors::ui::INPUT_BORDER,
        );
        graphics.fill_rect(
            content_x,
            content_y,
            content_width,
            output_height,
            colors::retro_theme::BACKGROUND,
        );
        graphics.fill_rect(
            content_x,
            input_y,
            content_width,
            INPUT_HEIGHT,
            colors::ui::INPUT_BACKGROUND,
        );

        let start_line = self.history.len().saturating_sub(visible_lines);
        for (index, line) in self.history.iter().skip(start_line).enumerate() {
            let y = content_y + index as u64 * LINE_HEIGHT;
            fonts::draw_string(
                graphics,
                content_x + 4,
                y,
                line,
                colors::retro_theme::TEXT,
            );
        }

        let prompt = format!("> {}", self.input);
        let rendered_prompt = if prompt.len() > max_chars {
            let start = prompt.len() - max_chars;
            &prompt[start..]
        } else {
            &prompt
        };

        fonts::draw_string(
            graphics,
            content_x + 4,
            input_y + 8,
            rendered_prompt,
            colors::ui::INPUT_TEXT,
        );

        if is_focused {
            let cursor_x = content_x + 4 + (rendered_prompt.len() as u64 * CHAR_WIDTH);
            graphics.fill_rect(
                cursor_x,
                input_y + 6,
                2,
                LINE_HEIGHT.saturating_sub(2),
                colors::retro_theme::CURSOR,
            );
        }
    }

    fn handle_key(&mut self, ch: u8) -> bool {
        match ch {
            8 => {
                self.input.pop();
                true
            }
            b'\n' | b'\r' => {
                self.submit_command();
                true
            }
            b'\t' => {
                self.input.push(' ');
                self.input.push(' ');
                self.input.push(' ');
                self.input.push(' ');
                true
            }
            32..=126 => {
                self.input.push(ch as char);
                true
            }
            _ => false,
        }
    }

    fn print_banner(&mut self) {
        self.push_line("OxideOS Terminal");
        self.push_line("Type 'help' to list available commands.");
        self.push_line("");
    }

    fn submit_command(&mut self) {
        let command = String::from(self.input.trim());
        self.push_line(&format!("> {}", self.input));
        self.input.clear();

        if command.is_empty() {
            return;
        }

        self.execute_command(&command);
    }

    fn execute_command(&mut self, command: &str) {
        if command == "help" {
            self.push_line("Commands:");
            self.push_line("  help      - show this help");
            self.push_line("  clear     - clear terminal history");
            self.push_line("  echo TXT  - print text");
            self.push_line("  ticks     - show timer ticks");
            self.push_line("  uptime    - show uptime in ms");
            self.push_line("  pid       - test getpid syscall");
            self.push_line("  about     - show terminal info");
            return;
        }

        if command == "clear" {
            self.history.clear();
            return;
        }

        if let Some(text) = command.strip_prefix("echo ") {
            self.push_line(text);
            return;
        }

        if command == "ticks" {
            let ticks = unsafe { timer::get_ticks() };
            self.push_line(&format!("ticks: {}", ticks));
            return;
        }

        if command == "uptime" {
            let ticks = unsafe { timer::get_ticks() };
            self.push_line(&format!("uptime: {} ms", ticks * 10));
            return;
        }

        if command == "pid" {
            let result = unsafe { syscall::handle_syscall(syscall::Syscall::GetPid as u64, 0, 0, 0, 0, 0) };
            self.push_line(&format!("pid: {}", result.value));
            return;
        }

        if command == "about" {
            self.push_line("OxideOS GUI terminal");
            self.push_line("Status: in-kernel command console");
            self.push_line("Next: user-space shell + binary loading");
            return;
        }

        self.push_line(&format!("unknown command: {}", command));
        self.push_line("Try 'help'.");
    }

    fn push_line(&mut self, text: &str) {
        if text.is_empty() {
            self.history.push(String::new());
            self.trim_history();
            return;
        }

        let max_chars = 72usize;
        let bytes = text.as_bytes();
        let mut start = 0usize;

        while start < bytes.len() {
            let end = (start + max_chars).min(bytes.len());
            self.history.push(String::from(&text[start..end]));
            start = end;
        }

        self.trim_history();
    }

    fn trim_history(&mut self) {
        while self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
    }
}
