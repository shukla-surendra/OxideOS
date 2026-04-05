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
const COMMAND_HISTORY_LIMIT: usize = 32;
const INPUT_QUEUE_SIZE: usize = 256;
const CHAR_WIDTH: u64 = 9;
const LINE_HEIGHT: u64 = 16;
const CONTENT_PADDING_X: u64 = 10;
const CONTENT_PADDING_Y: u64 = 8;
const INPUT_HEIGHT: u64 = 24;

const EVENT_ARROW_UP: u16 = 0x100;
const EVENT_ARROW_DOWN: u16 = 0x101;
const EVENT_ARROW_LEFT: u16 = 0x102;
const EVENT_ARROW_RIGHT: u16 = 0x103;

const COMMANDS: &[&str] = &[
    "about",
    "clear",
    "cls",
    "echo",
    "help",
    "history",
    "pid",
    "sysinfo",
    "ticks",
    "uptime",
    "version",
];

static mut INPUT_QUEUE: [u16; INPUT_QUEUE_SIZE] = [0; INPUT_QUEUE_SIZE];
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

unsafe fn queue_event(event: u16) {
    let next_tail = (INPUT_TAIL + 1) % INPUT_QUEUE_SIZE;
    if next_tail != INPUT_HEAD {
        INPUT_QUEUE[INPUT_TAIL] = event;
        INPUT_TAIL = next_tail;
    }
}

fn dequeue_event() -> Option<u16> {
    with_interrupts_disabled(|| unsafe {
        if INPUT_HEAD == INPUT_TAIL {
            None
        } else {
            let event = INPUT_QUEUE[INPUT_HEAD];
            INPUT_HEAD = (INPUT_HEAD + 1) % INPUT_QUEUE_SIZE;
            Some(event)
        }
    })
}

unsafe fn terminal_key_callback(ch: u8) {
    queue_event(ch as u16);
}

unsafe fn terminal_arrow_callback(key: keyboard::ArrowKey) {
    let event = match key {
        keyboard::ArrowKey::Up => EVENT_ARROW_UP,
        keyboard::ArrowKey::Down => EVENT_ARROW_DOWN,
        keyboard::ArrowKey::Left => EVENT_ARROW_LEFT,
        keyboard::ArrowKey::Right => EVENT_ARROW_RIGHT,
    };
    queue_event(event);
}

pub unsafe fn install_input_hooks() {
    keyboard::register_key_callback(terminal_key_callback);
    keyboard::register_arrow_key_callback(terminal_arrow_callback);
}

pub struct TerminalApp {
    window_id: usize,
    history: Vec<String>,
    input: String,
    command_history: Vec<String>,
    history_cursor: Option<usize>,
}

impl TerminalApp {
    pub fn new(window_id: usize) -> Self {
        let mut terminal = Self {
            window_id,
            history: Vec::new(),
            input: String::new(),
            command_history: Vec::new(),
            history_cursor: None,
        };
        terminal.print_banner();
        terminal
    }

    pub fn window_id(&self) -> usize {
        self.window_id
    }

    pub fn process_pending_input(&mut self, focused: bool) -> bool {
        let mut changed = false;

        while let Some(event) = dequeue_event() {
            if focused {
                changed |= self.handle_event(event);
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

    fn handle_event(&mut self, event: u16) -> bool {
        match event {
            EVENT_ARROW_UP => self.history_up(),
            EVENT_ARROW_DOWN => self.history_down(),
            EVENT_ARROW_LEFT | EVENT_ARROW_RIGHT => false,
            _ => self.handle_key(event as u8),
        }
    }

    fn handle_key(&mut self, ch: u8) -> bool {
        match ch {
            8 => {
                self.history_cursor = None;
                self.input.pop();
                true
            }
            b'\n' | b'\r' => {
                self.submit_command();
                true
            }
            b'\t' => self.autocomplete(),
            32..=126 => {
                self.history_cursor = None;
                self.input.push(ch as char);
                true
            }
            _ => false,
        }
    }

    fn print_banner(&mut self) {
        self.push_line("OxideOS Terminal");
        self.push_line("Type 'help' to list available commands.");
        self.push_line("Tips: Up/Down browse history, Tab completes commands.");
        self.push_line("");
    }

    fn submit_command(&mut self) {
        let command = String::from(self.input.trim());
        self.push_line(&format!("> {}", self.input));
        self.input.clear();
        self.history_cursor = None;

        if command.is_empty() {
            return;
        }

        self.record_command(&command);
        self.execute_command(&command);
    }

    fn execute_command(&mut self, command: &str) {
        let mut parts = command.split_whitespace();
        let Some(name) = parts.next() else {
            return;
        };

        match name {
            "help" => {
                self.push_line("Commands:");
                self.push_line("  help        - show this help");
                self.push_line("  clear | cls - clear terminal output");
                self.push_line("  echo TXT    - print text");
                self.push_line("  ticks       - show timer ticks");
                self.push_line("  uptime      - show uptime in ms");
                self.push_line("  pid         - test getpid syscall");
                self.push_line("  sysinfo     - show kernel system info");
                self.push_line("  history     - show command history");
                self.push_line("  version     - show build banner");
                self.push_line("  about       - show terminal info");
            }
            "clear" | "cls" => {
                self.history.clear();
            }
            "echo" => {
                let text = command.strip_prefix("echo").unwrap_or("").trim_start();
                self.push_line(text);
            }
            "ticks" => {
                let ticks = unsafe { timer::get_ticks() };
                self.push_line(&format!("ticks: {}", ticks));
            }
            "uptime" => {
                let ticks = unsafe { timer::get_ticks() };
                self.push_line(&format!("uptime: {} ms", ticks * 10));
            }
            "pid" => {
                let result =
                    unsafe { syscall::handle_syscall(syscall::Syscall::GetPid as u64, 0, 0, 0, 0, 0) };
                self.push_line(&format!("pid: {}", result.value));
            }
            "sysinfo" => {
                let info = syscall::snapshot_system_info();
                self.push_line("system:");
                self.push_line(&format!("  uptime_ms: {}", info.uptime_ms));
                self.push_line(&format!("  total_memory: {} MB", info.total_memory / 1024 / 1024));
                self.push_line(&format!("  free_memory: {} MB", info.free_memory / 1024 / 1024));
                self.push_line(&format!("  process_count: {}", info.process_count));
            }
            "history" => {
                if self.command_history.is_empty() {
                    self.push_line("history: empty");
                } else {
                    self.push_line("command history:");
                    let start = self.command_history.len().saturating_sub(10);
                    let entries: Vec<String> = self.command_history
                        .iter()
                        .enumerate()
                        .skip(start)
                        .map(|(index, entry)| format!("  {:02}: {}", index + 1, entry))
                        .collect();
                    for entry in entries {
                        self.push_line(&entry);
                    }
                }
            }
            "version" => {
                self.push_line("OxideOS kernel prototype");
                self.push_line("GUI terminal revision 2");
            }
            "about" => {
                self.push_line("OxideOS GUI terminal");
                self.push_line("Status: in-kernel command console");
                self.push_line("Features: history, tab complete, system info");
                self.push_line("Next: user-space shell + binary loading");
            }
            _ => {
                self.push_line(&format!("unknown command: {}", command));
                self.push_line("Try 'help'.");
            }
        }
    }

    fn record_command(&mut self, command: &str) {
        if self.command_history.last().map(|entry| entry.as_str()) != Some(command) {
            self.command_history.push(String::from(command));
            while self.command_history.len() > COMMAND_HISTORY_LIMIT {
                self.command_history.remove(0);
            }
        }
    }

    fn history_up(&mut self) -> bool {
        if self.command_history.is_empty() {
            return false;
        }

        let next_index = match self.history_cursor {
            Some(index) if index > 0 => index - 1,
            Some(0) => 0,
            None => self.command_history.len() - 1,
            _ => return false,
        };

        self.history_cursor = Some(next_index);
        self.input = self.command_history[next_index].clone();
        true
    }

    fn history_down(&mut self) -> bool {
        match self.history_cursor {
            Some(index) if index + 1 < self.command_history.len() => {
                self.history_cursor = Some(index + 1);
                self.input = self.command_history[index + 1].clone();
                true
            }
            Some(_) => {
                self.history_cursor = None;
                self.input.clear();
                true
            }
            None => false,
        }
    }

    fn autocomplete(&mut self) -> bool {
        let prefix = String::from(self.input.trim());
        if prefix.is_empty() || prefix.contains(' ') {
            return false;
        }

        let matches: Vec<&str> = COMMANDS
            .iter()
            .copied()
            .filter(|cmd| cmd.starts_with(prefix.as_str()))
            .collect();

        let Some(first) = matches.first().copied() else {
            self.push_line("no completions");
            return true;
        };

        if matches.len() > 1 {
            self.push_line("matches:");
            for entry in matches {
                self.push_line(&format!("  {}", entry));
            }
            return true;
        }

        self.input = String::from(first);
        if matches_argument(first) {
            self.input.push(' ');
        }
        true
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

fn matches_argument(command: &str) -> bool {
    matches!(command, "echo")
}
