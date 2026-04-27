//! GUI terminal for OxideOS.
//!
//! In-kernel command console with RamFS integration.
//! Supports file commands: ls, cat, mkdir, touch, write, rm, pwd.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;

use crate::kernel::{keyboard, syscall, timer};
use crate::kernel::fs::ramfs::{RAMFS, NodeKind};

use super::colors;
use super::fonts;
use super::graphics::Graphics;
use super::window_manager::WindowManager;

// ── Layout constants ───────────────────────────────────────────────────────
const HISTORY_LIMIT:        usize = 160;
const COMMAND_HISTORY_LIMIT: usize = 32;
const INPUT_QUEUE_SIZE:     usize = 256;
const CHAR_WIDTH:   u64 = 9;
const LINE_HEIGHT:  u64 = 16;
const CONTENT_PADDING_X: u64 = 10;
const CONTENT_PADDING_Y: u64 = 8;
const INPUT_HEIGHT: u64 = 24;

// ── Special key events ─────────────────────────────────────────────────────
const EVENT_ARROW_UP:    u16 = 0x100;
const EVENT_ARROW_DOWN:  u16 = 0x101;
const EVENT_ARROW_LEFT:  u16 = 0x102;
const EVENT_ARROW_RIGHT: u16 = 0x103;
const EVENT_PAGE_UP:     u16 = 0x104;
const EVENT_PAGE_DOWN:   u16 = 0x105;

const SCROLL_AMOUNT: usize = 5;

// ── Tab-completion word list ───────────────────────────────────────────────
const COMMANDS: &[&str] = &[
    "about", "cat", "cd", "clear", "cls", "diskinfo", "echo",
    "help", "history", "kill", "ls", "mkdir", "pid",
    "ps", "pwd", "reboot", "record", "rm", "run", "sh", "shutdown", "sysinfo",
    "terminal", "ticks", "touch", "uptime", "version", "write",
];

// ── Input queue (interrupt-safe ring buffer) ───────────────────────────────
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
        unsafe { asm!("sti", options(nomem, preserves_flags)); }
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
        if INPUT_HEAD == INPUT_TAIL { return None; }
        let event = INPUT_QUEUE[INPUT_HEAD];
        INPUT_HEAD = (INPUT_HEAD + 1) % INPUT_QUEUE_SIZE;
        Some(event)
    })
}

/// Push a VT100 cursor-key escape sequence (\x1B [ X) into the stdin ring
/// so a running user program (e.g. the shell) can handle arrow keys.
fn push_vt100(suffix: u8) {
    crate::kernel::stdin::push(0x1B);
    crate::kernel::stdin::push(b'[');
    crate::kernel::stdin::push(suffix);
}

/// Pick a colour for a terminal history line based on its prefix/content.
fn line_color(line: &str) -> u32 {
    if line.starts_with("[info]") || line.starts_with("[ok]") {
        0xFF4EC9B0  // teal
    } else if line.starts_with("[error]") || line.starts_with("error:") {
        0xFFF14C4C  // red
    } else if line.starts_with("[warn]") {
        0xFFDDBB00  // amber
    } else if line.starts_with("> ") {
        0xFF608060  // dim green — echoed command
    } else if line.starts_with("  ") && !line.trim().is_empty() {
        0xFF888888  // medium gray — indented output / ls entries
    } else if line.starts_with("exited") || line.starts_with("spawned") {
        0xFF4EC9B0  // teal — process lifecycle
    } else if line.starts_with('[') && line.contains("] exited") {
        0xFF888888  // dim — background exit
    } else if line.starts_with("  ___") || line.starts_with(" / _")
           || line.starts_with("| |")  || line.starts_with(" \\___")
           || line.starts_with(" \\___")|| line.starts_with("| (_)")
           || line.starts_with(" \\___"){
        0xFF4EC9B0  // teal — ASCII art banner
    } else if line.is_empty() {
        0xFF0C0C0C  // background — blank lines
    } else {
        0xFFCCCCCC  // default: light gray
    }
}

unsafe fn terminal_key_callback(ch: u8) { unsafe { queue_event(ch as u16); } }

unsafe fn terminal_arrow_callback(key: keyboard::ArrowKey) {
    let event = match key {
        keyboard::ArrowKey::Up       => EVENT_ARROW_UP,
        keyboard::ArrowKey::Down     => EVENT_ARROW_DOWN,
        keyboard::ArrowKey::Left     => EVENT_ARROW_LEFT,
        keyboard::ArrowKey::Right    => EVENT_ARROW_RIGHT,
        keyboard::ArrowKey::PageUp   => EVENT_PAGE_UP,
        keyboard::ArrowKey::PageDown => EVENT_PAGE_DOWN,
    };
    unsafe { queue_event(event); }
}

pub unsafe fn install_input_hooks() {
    keyboard::register_key_callback(terminal_key_callback);
    keyboard::register_arrow_key_callback(terminal_arrow_callback);
}

/// Pop one raw key event from the shared keyboard ring.
/// Returns the encoded event: plain chars as u16, arrow keys as 0x100+.
/// Other kernel GUI widgets (Notepad, etc.) call this when they are focused.
pub fn pop_key_event() -> Option<u16> {
    dequeue_event()
}

// ============================================================================
// TERMINAL APPLICATION
// ============================================================================

pub struct TerminalApp {
    window_id:        usize,
    history:          Vec<String>,
    input:            String,
    /// Byte-index of the cursor within `input` (0 = before first char).
    cursor_pos:       usize,
    command_history:  Vec<String>,
    history_cursor:   Option<usize>,
    cwd:              String,
    /// True while a user program owns the terminal (fork/exec).
    passthrough_mode: bool,
    /// PID of the currently-running foreground program, if any.
    fg_pid:           Option<u8>,
    /// Characters typed while a program is running (for visual echo only).
    passthrough_input: String,
    /// Lines scrolled back from the bottom (0 = follow tail).
    scroll_offset:    usize,
    /// When true the compositor (userspace terminal) owns this window's content
    /// area. draw() becomes a no-op so it never overwrites compositor output.
    /// The kernel terminal stays as a fallback — if the userspace process exits
    /// this is cleared and the kernel terminal takes over automatically.
    compositor_mode:  bool,
}

impl TerminalApp {
    pub fn new(window_id: usize) -> Self {
        let mut t = Self {
            window_id,
            history:          Vec::new(),
            input:            String::new(),
            cursor_pos:       0,
            command_history:  Vec::new(),
            history_cursor:   None,
            cwd:              String::from("/"),
            passthrough_mode: false,
            fg_pid:           None,
            passthrough_input: String::new(),
            scroll_offset:    0,
            compositor_mode:  false,
        };
        t.print_banner();
        t
    }

    /// Hand this window's content area over to the compositor.
    /// While enabled, draw() is suppressed so the compositor output is never
    /// overwritten. If the userspace process exits, on_task_exit() clears this
    /// automatically and the kernel terminal fallback takes over.
    pub fn set_compositor_mode(&mut self, enabled: bool) {
        self.compositor_mode = enabled;
    }

    pub fn is_compositor_mode(&self) -> bool {
        self.compositor_mode
    }

    /// Resolve a path relative to cwd.  Handles `..` and leading `/`.
    fn resolve_path(&self, input: &str) -> String {
        let base = if input.starts_with('/') {
            String::from(input)
        } else if self.cwd == "/" {
            format!("/{}", input)
        } else {
            format!("{}/{}", self.cwd.trim_end_matches('/'), input)
        };

        // Normalise `.` and `..` segments
        let mut parts: Vec<&str> = Vec::new();
        for seg in base.split('/') {
            match seg {
                "" | "." => {}
                ".." => { parts.pop(); }
                s    => parts.push(s),
            }
        }
        if parts.is_empty() {
            String::from("/")
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    /// Returns `true` when path lives on the FAT disk (`/disk` or `/disk/…`).
    fn is_disk_path(path: &str) -> bool {
        path == "/disk" || path.starts_with("/disk/")
    }

    /// Read a FAT file and push its lines into the history buffer.
    fn cat_fat(&mut self, path: &str) {
        // fat::open expects raw bytes; strip "/disk" prefix for the driver
        let fat_path = if path.starts_with("/disk/") { &path[6..] }
                       else if path == "/disk"        { "" }
                       else                           { path };
        let fat_path_bytes = fat_path.as_bytes();

        let fd = unsafe { crate::kernel::fat::open(fat_path_bytes, 0) };
        if fd < 0 {
            self.push_line(&format!("cat: {}: no such file", path));
            return;
        }
        let fd = fd as i32;

        let mut all_data: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let n = unsafe { crate::kernel::fat::read_fd(fd, &mut chunk) };
            if n <= 0 { break; }
            all_data.extend_from_slice(&chunk[..n as usize]);
            if all_data.len() > 65536 { self.push_line("(truncated at 64 KB)"); break; }
        }
        unsafe { crate::kernel::fat::close(fd); }

        if all_data.is_empty() {
            self.push_line("(empty file)");
        } else {
            match core::str::from_utf8(&all_data) {
                Ok(text) => { for line in text.lines() { self.push_line(line); } }
                Err(_)   => { self.push_line(&format!("(binary, {} bytes)", all_data.len())); }
            }
        }
    }

    pub fn window_id(&self) -> usize { self.window_id }

    /// Called by the main loop when a task exits.
    /// Drains any remaining stdout and shows the exit status.
    pub fn on_task_exit(&mut self, pid: u8, exit_code: i64) {
        let was_compositor_fg = self.compositor_mode && self.fg_pid == Some(pid);

        // Final drain of any buffered output
        let mut lines: Vec<String> = Vec::new();
        crate::kernel::scheduler::output_drain_task(
            (pid as usize).saturating_sub(1),
            |line| lines.push(String::from(line)),
        );
        for line in &lines { self.push_line(line); }

        if self.fg_pid == Some(pid) {
            self.exit_passthrough();
        }
        if exit_code != 0 {
            self.push_line(&format!("[pid {}] exited: {}", pid, exit_code));
        }

        // If the userspace terminal process died, hand the window back to the
        // kernel terminal so the user still has a working shell.
        if was_compositor_fg {
            self.compositor_mode = false;
            self.push_line("[info] Userspace terminal exited — kernel terminal active.");
            self.push_line("[info] Type 'run terminal' to relaunch, or use built-in commands.");
        }
    }

    /// Drain stdout from all running tasks into history. Returns true if any
    /// new output was flushed (so the caller can trigger a redraw).
    pub fn poll_task_outputs(&mut self) -> bool {
        let mut lines: Vec<String> = Vec::new();
        for idx in 0..crate::kernel::scheduler::MAX_TASKS {
            crate::kernel::scheduler::output_drain_task(idx, |line| {
                lines.push(String::from(line));
            });
        }
        let any = !lines.is_empty();
        for line in &lines { self.push_line(line); }
        any
    }

    /// Attach a spawned process as the foreground program for this terminal.
    pub fn attach_foreground(&mut self, pid: u8) { self.enter_passthrough(pid); }

    fn enter_passthrough(&mut self, pid: u8) {
        self.passthrough_mode  = true;
        self.fg_pid            = Some(pid);
        self.passthrough_input.clear();
    }

    fn exit_passthrough(&mut self) {
        self.passthrough_mode = false;
        self.fg_pid           = None;
        self.passthrough_input.clear();
    }

    pub fn process_pending_input(&mut self, focused: bool) -> bool {
        let mut changed = false;
        while let Some(event) = dequeue_event() {
            if self.passthrough_mode {
                match event {
                    // Ctrl+C — kill foreground process
                    3 => {
                        if let Some(pid) = self.fg_pid {
                            unsafe { crate::kernel::scheduler::kill(pid); }
                            self.push_line(&format!("[pid {}] killed (Ctrl+C)", pid));
                            self.exit_passthrough();
                            changed = true;
                        }
                    }
                    // Scroll history while program is running
                    EVENT_PAGE_UP => {
                        let max = self.history.len().saturating_sub(1);
                        self.scroll_offset = (self.scroll_offset + SCROLL_AMOUNT).min(max);
                        changed = true;
                    }
                    EVENT_PAGE_DOWN => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(SCROLL_AMOUNT);
                        changed = true;
                    }
                    // Arrow keys → push VT100 escape sequence into stdin for the running program
                    EVENT_ARROW_UP    => { push_vt100(b'A'); }
                    EVENT_ARROW_DOWN  => { push_vt100(b'B'); }
                    EVENT_ARROW_RIGHT => { push_vt100(b'C'); }
                    EVENT_ARROW_LEFT  => { push_vt100(b'D'); }
                    // Backspace: remove last char from echo buffer (key already in stdin)
                    8 => {
                        self.passthrough_input.pop();
                        changed = true;
                    }
                    // Enter/newline: clear echo buffer (newline already sent to stdin)
                    10 | 13 => {
                        self.passthrough_input.clear();
                        changed = true;
                    }
                    // Printable chars: append to echo buffer for display
                    32..=126 => {
                        self.passthrough_input.push(event as u8 as char);
                        changed = true;
                    }
                    _ => {}
                }
            } else if focused {
                changed |= self.handle_event(event);
            }
        }
        changed
    }

    // ── Drawing ─────────────────────────────────────────────────────────────
    //
    // Layout (top → bottom inside the WM window):
    //   [Title bar — drawn by WindowManager]
    //   [Status bar  18px] — brand name left, uptime right
    //   [1px separator]
    //   [History lines …] — scrollable, newest at bottom
    //   [Prompt + input  ]  — last row, no box
    //
    // A 6px scrollbar track lives on the right edge of the text area.
    // A 2px focus accent runs on the left edge.

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager) {
        // Compositor owns this window's content — never overwrite it.
        if self.compositor_mode { return; }
        if !wm.is_window_visible(self.window_id) { return; }
        let Some(window) = wm.get_window(self.window_id) else { return; };

        let is_focused = wm.get_focused() == Some(self.window_id);

        // ── Geometry ──────────────────────────────────────────────────────
        let cx = window.x + 1;
        let cy = window.y + 31;
        let cw = window.width.saturating_sub(2);
        let ch = window.height.saturating_sub(32);

        // ── Colour palette (Ubuntu Terminal inspired) ─────────────────────
        const BG:         u32 = 0xFF0D0D0D; // near-black
        const STATUS_BG:  u32 = 0xFF2C001E; // Ubuntu aubergine strip
        const STATUS_SEP: u32 = 0xFF4A0030;
        const C_BRAND:    u32 = 0xFFE95420; // Ubuntu orange
        const C_UPTIME:   u32 = 0xFF7A4050;
        const C_USER:     u32 = 0xFF4CE046; // bright green (user@host)
        const C_CWD_COL:  u32 = 0xFF5FC5E0; // cyan path
        const C_DIM:      u32 = 0xFF555555;
        const C_SCROLL:   u32 = 0xFF4A4A4A; // scrollbar thumb
        const C_SCROLL_T: u32 = 0xFF1A1A1A; // scrollbar track
        const C_INPUT:    u32 = 0xFFEEEEEE; // typed text
        const C_CURSOR:   u32 = 0xFFFFFFFF;
        const C_PASS:     u32 = 0xFF3A3A3A; // passthrough label

        // ── Status bar ────────────────────────────────────────────────────
        const SBAR_H: u64 = 18;
        graphics.fill_rect(cx, cy, cw, SBAR_H, STATUS_BG);
        graphics.fill_rect(cx, cy + SBAR_H, cw, 1, STATUS_SEP);

        fonts::draw_string(graphics, cx + 8, cy + 3, "oxide-terminal", C_BRAND);

        // Uptime HH:MM:SS on the right
        let ticks = unsafe { timer::get_ticks() };
        let secs  = ticks / 100;
        let mut tbuf = [0u8; 8];
        tbuf[0] = b'0' + ((secs / 3600 / 10) % 10) as u8;
        tbuf[1] = b'0' + ((secs / 3600)      % 10) as u8;
        tbuf[2] = b':';
        tbuf[3] = b'0' + ((secs / 60 / 10)   % 6)  as u8;
        tbuf[4] = b'0' + ((secs / 60)         % 10) as u8;
        tbuf[5] = b':';
        tbuf[6] = b'0' + ((secs / 10)         % 6)  as u8;
        tbuf[7] = b'0' + (secs                % 10) as u8;
        if let Ok(ts) = core::str::from_utf8(&tbuf) {
            let rx = cx + cw.saturating_sub(ts.len() as u64 * CHAR_WIDTH + 8);
            fonts::draw_string(graphics, rx, cy + 3, ts, C_UPTIME);
        }

        // ── Text area ─────────────────────────────────────────────────────
        const SB_W:   u64 = 6;  // scrollbar width
        const ACC_W:  u64 = 2;  // left focus accent width
        const MARGIN: u64 = 6;  // left text margin (after accent)

        let text_top = cy + SBAR_H + 1;
        let text_h   = ch.saturating_sub(SBAR_H + 1);

        // Background with rounded bottom (matching window)
        graphics.fill_rounded_rect(cx, text_top, cw, text_h, 8, BG);
        // Cover top part of rounding
        graphics.fill_rect(cx, text_top, cw, 10, BG);

        // Focus accent (left edge)
        let accent_col = if is_focused { C_USER } else { 0xFF1E1E1E };
        graphics.fill_rect(cx, text_top, ACC_W, text_h - 8, accent_col);

        // Scrollbar track
        graphics.fill_rect(cx + cw - SB_W, text_top, SB_W, text_h - 8, C_SCROLL_T);


        // Usable text column metrics
        let text_x   = cx + ACC_W + MARGIN;
        let usable_w = cw.saturating_sub(ACC_W + MARGIN + SB_W + 2);
        let max_cols = (usable_w / CHAR_WIDTH).max(4) as usize;

        let total_rows   = (text_h / LINE_HEIGHT).max(2) as usize;
        let history_rows = total_rows.saturating_sub(1); // last row = prompt

        // ── Scroll-back banner ────────────────────────────────────────────
        let banner_row = if self.scroll_offset > 0 {
            let above = self.history.len()
                .saturating_sub(history_rows + self.scroll_offset);
            let msg = format!(" \u{2191} {} lines above  (PgDn to scroll down) ",
                              above + self.scroll_offset);
            graphics.fill_rect(cx + ACC_W, text_top, cw.saturating_sub(ACC_W + SB_W), LINE_HEIGHT, 0xFF181828);
            let d = if msg.len() > max_cols { &msg[..max_cols] } else { msg.as_str() };
            fonts::draw_string(graphics, text_x, text_top + 2, d, 0xFF555580);
            1usize // first history row is taken by the banner
        } else {
            0usize
        };

        // ── History ───────────────────────────────────────────────────────
        let end_idx   = self.history.len().saturating_sub(self.scroll_offset);
        let avail_rows = history_rows.saturating_sub(banner_row);
        let start_idx = end_idx.saturating_sub(avail_rows);

        for (i, line) in self.history.iter().skip(start_idx).take(avail_rows).enumerate() {
            let row = banner_row + i;
            let y   = text_top + row as u64 * LINE_HEIGHT + 2;
            let col = line_color(line);
            let d   = if line.len() > max_cols { &line[..max_cols] } else { line.as_str() };
            fonts::draw_string(graphics, text_x, y, d, col);
        }

        // ── Scrollbar thumb ───────────────────────────────────────────────
        if self.history.len() > history_rows {
            let total      = self.history.len() as u64;
            let vis        = history_rows as u64;
            let thumb_h    = (text_h * vis / total).max(12).min(text_h);
            let scroll_max = total.saturating_sub(vis);
            let at_bottom  = scroll_max.saturating_sub(self.scroll_offset as u64);
            let thumb_top  = if scroll_max > 0 {
                text_top + (text_h - thumb_h) * at_bottom / scroll_max
            } else {
                text_top + text_h - thumb_h
            };
            graphics.fill_rect(cx + cw - SB_W + 1, thumb_top, SB_W - 2, thumb_h, C_SCROLL);
        }

        // ── Prompt / input (always last row) ──────────────────────────────
        let prompt_row_y = text_top + history_rows as u64 * LINE_HEIGHT + 2;

        if self.passthrough_mode {
            // A program owns stdin — show dim PID label + typed echo
            let label: String = match self.fg_pid {
                Some(p) => format!("[{}] ", p),
                None    => String::new(),
            };
            let label_len = label.len();
            fonts::draw_string(graphics, text_x, prompt_row_y, &label, C_PASS);

            let echo_x    = text_x + label_len as u64 * CHAR_WIDTH;
            let avail     = max_cols.saturating_sub(label_len);
            let raw       = &self.passthrough_input;
            let eo        = raw.len().saturating_sub(avail);
            let vis_echo  = &raw[eo..];
            fonts::draw_string(graphics, echo_x, prompt_row_y, vis_echo, C_INPUT);

            // Thin I-beam cursor at end of echo
            if is_focused {
                let cur_x = echo_x + vis_echo.len() as u64 * CHAR_WIDTH;
                graphics.fill_rect(cur_x, prompt_row_y - 1, 2, LINE_HEIGHT, C_USER);
            }
        } else {
            // ── Bash-style multi-colour prompt: oxide : path $ ─────────────
            let short_cwd = if self.cwd == "/" || self.cwd.is_empty() {
                "~"
            } else {
                self.cwd.trim_end_matches('/')
            };

            let mut px = text_x;

            // "oxide@os" in green (Ubuntu-style user@host)
            fonts::draw_string(graphics, px, prompt_row_y, "oxide@os", C_USER);
            px += 8 * CHAR_WIDTH;

            // ":" in dim
            fonts::draw_string(graphics, px, prompt_row_y, ":", C_DIM);
            px += CHAR_WIDTH;

            // path in cyan
            fonts::draw_string(graphics, px, prompt_row_y, short_cwd, C_CWD_COL);
            px += short_cwd.len() as u64 * CHAR_WIDTH;

            // "$ " in white
            fonts::draw_string(graphics, px, prompt_row_y, "$ ", 0xFFFFFFFF);
            px += 2 * CHAR_WIDTH;

            let prompt_cols = 8 + 1 + short_cwd.len() + 2;
            let avail_input = max_cols.saturating_sub(prompt_cols);

            // Scroll window so cursor stays visible
            let win_start = if self.cursor_pos > avail_input {
                self.cursor_pos - avail_input
            } else {
                0
            };
            let win_start = {
                let mut s = win_start;
                while s > 0 && !self.input.is_char_boundary(s) { s -= 1; }
                s
            };
            let vis_slice: &str = &self.input[win_start..];
            let display: &str = if vis_slice.len() > avail_input {
                &vis_slice[..avail_input]
            } else {
                vis_slice
            };

            fonts::draw_string(graphics, px, prompt_row_y, display, C_INPUT);

            // Block cursor
            if is_focused {
                let chars_before = self.cursor_pos.saturating_sub(win_start);
                let cur_x        = px + chars_before as u64 * CHAR_WIDTH;
                graphics.fill_rect(cur_x, prompt_row_y - 1, CHAR_WIDTH, LINE_HEIGHT, C_CURSOR);
                // Char under cursor in BG so it stays readable
                if let Some(ch) = self.input[self.cursor_pos..].chars().next() {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    fonts::draw_string(graphics, cur_x, prompt_row_y, s, BG);
                }
            }
        }
    }

    // ── Input handling ───────────────────────────────────────────────────────

    fn handle_event(&mut self, event: u16) -> bool {
        match event {
            EVENT_ARROW_UP    => self.history_up(),
            EVENT_ARROW_DOWN  => self.history_down(),
            EVENT_ARROW_LEFT  => {
                if self.cursor_pos > 0 {
                    // Step back one UTF-8 character boundary
                    self.cursor_pos -= 1;
                    while self.cursor_pos > 0
                        && !self.input.is_char_boundary(self.cursor_pos)
                    {
                        self.cursor_pos -= 1;
                    }
                    true
                } else {
                    false
                }
            }
            EVENT_ARROW_RIGHT => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                    while self.cursor_pos < self.input.len()
                        && !self.input.is_char_boundary(self.cursor_pos)
                    {
                        self.cursor_pos += 1;
                    }
                    true
                } else {
                    false
                }
            }
            EVENT_PAGE_UP     => {
                let max_scroll = self.history.len().saturating_sub(1);
                self.scroll_offset = (self.scroll_offset + SCROLL_AMOUNT).min(max_scroll);
                true
            }
            EVENT_PAGE_DOWN   => {
                self.scroll_offset = self.scroll_offset.saturating_sub(SCROLL_AMOUNT);
                true
            }
            _                 => self.handle_key(event as u8),
        }
    }

    fn handle_key(&mut self, ch: u8) -> bool {
        match ch {
            8 => {
                // Backspace: delete char before cursor
                if self.cursor_pos > 0 {
                    self.history_cursor = None;
                    // Find the start of the char just before cursor_pos
                    let mut start = self.cursor_pos - 1;
                    while start > 0 && !self.input.is_char_boundary(start) {
                        start -= 1;
                    }
                    self.input.drain(start..self.cursor_pos);
                    self.cursor_pos = start;
                }
                true
            }
            b'\n' | b'\r' => { self.submit_command(); true }
            b'\t'         => self.autocomplete(),
            32..=126      => {
                self.history_cursor = None;
                self.input.insert(self.cursor_pos, ch as char);
                self.cursor_pos += 1;
                true
            }
            _ => false,
        }
    }

    fn print_banner(&mut self) {
        self.push_line("");
        self.push_line("[info] Type 'help' for commands  |  Tab to complete  |  PgUp/PgDn to scroll");
        self.push_line("[info] Run programs with: run <name>   List programs: run");
        self.push_line("");
    }

    fn submit_command(&mut self) {
        let command = String::from(self.input.trim());
        self.push_line(&format!("> {}", self.input));
        self.input.clear();
        self.cursor_pos    = 0;
        self.history_cursor = None;
        if command.is_empty() { return; }
        self.record_command(&command);
        self.execute_command(&command);
    }

    // ── Command dispatcher ────────────────────────────────────────────────────

    fn execute_command(&mut self, command: &str) {
        let mut parts = command.split_whitespace();
        let Some(name) = parts.next() else { return; };

        match name {
            // ── Filesystem commands ──────────────────────────────────────────

            "cd" => {
                let target = parts.next().unwrap_or("/");
                let resolved = self.resolve_path(target);

                if resolved == "/" {
                    self.cwd = String::from("/");
                } else if Self::is_disk_path(&resolved) {
                    if !crate::kernel::ata::is_present() {
                        self.push_line("cd: no disk attached");
                    } else {
                        // Check if the resolved path is a real FAT directory.
                        let path_bytes = resolved.as_bytes();
                        let ok = if resolved == "/disk" || resolved == "/disk/" {
                            true
                        } else {
                            unsafe { crate::kernel::fat::resolve_dir(path_bytes).is_some() }
                        };
                        if ok {
                            let mut cwd = resolved;
                            if !cwd.ends_with('/') { cwd.push('/'); }
                            self.cwd = cwd;
                        } else {
                            self.push_line(&format!("cd: {}: no such directory", target));
                        }
                    }
                } else {
                    // RamFS directory check
                    let found = unsafe {
                        RAMFS.get().and_then(|fs| fs.list_dir(&resolved)).is_some()
                    };
                    if found {
                        self.cwd = resolved;
                    } else {
                        self.push_line(&format!("cd: {}: no such directory", target));
                    }
                }
            }

            "ls" => {
                let arg = parts.next();
                let path = match arg {
                    Some(p) => self.resolve_path(p),
                    None    => self.cwd.clone(),
                };

                if Self::is_disk_path(&path) {
                    if !crate::kernel::ata::is_present() {
                        self.push_line("ls: no disk attached");
                    } else {
                        // Resolve to a DirLoc (root or subdir).
                        let dir_loc = unsafe {
                            crate::kernel::fat::resolve_dir(path.as_bytes())
                        };
                        match dir_loc {
                            None => self.push_line("ls: no such directory"),
                            Some(loc) => {
                                let entries = unsafe { crate::kernel::fat::list_dir(loc) };
                                if entries.is_empty() {
                                    self.push_line("(empty directory)");
                                } else {
                                    for (name, is_dir) in &entries {
                                        let suffix = if *is_dir { "/" } else { "" };
                                        self.push_line(&format!("  {}{}", name, suffix));
                                    }
                                    self.push_line(&format!("  ({} entries)", entries.len()));
                                }
                            }
                        }
                    }
                } else {
                    // RamFS directory listing
                    unsafe {
                        match RAMFS.get() {
                            Some(fs) => match fs.list_dir(&path) {
                                Some(entries) => {
                                    if entries.is_empty() {
                                        self.push_line("(empty directory)");
                                    } else {
                                        for (name, kind) in &entries {
                                            let suffix = if *kind == NodeKind::Directory { "/" } else { "" };
                                            self.push_line(&format!("  {}{}", name, suffix));
                                        }
                                        self.push_line(&format!("  ({} entries)", entries.len()));
                                    }
                                }
                                None => self.push_line("ls: no such directory"),
                            },
                            None => self.push_line("ls: filesystem not ready"),
                        }
                    }
                }
            }

            "cat" => {
                match parts.next() {
                    None => self.push_line("usage: cat <path>"),
                    Some(arg) => {
                        let path = self.resolve_path(arg);

                        if Self::is_disk_path(&path) {
                            self.cat_fat(&path);
                        } else {
                            // RamFS read
                            let result = unsafe {
                                RAMFS.get().and_then(|fs| {
                                    fs.read_file(&path).map(|d| d.to_vec())
                                })
                            };
                            match result {
                                None => self.push_line("cat: no such file"),
                                Some(data) => {
                                    if data.is_empty() {
                                        self.push_line("(empty file)");
                                    } else {
                                        match core::str::from_utf8(&data) {
                                            Ok(text) => { for line in text.lines() { self.push_line(line); } }
                                            Err(_)   => { self.push_line(&format!("(binary, {} bytes)", data.len())); }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            "mkdir" => {
                match parts.next() {
                    None     => self.push_line("usage: mkdir <path>"),
                    Some(arg) => {
                        let path = self.resolve_path(arg);
                        if Self::is_disk_path(&path) {
                            if !crate::kernel::ata::is_present() {
                                self.push_line("mkdir: no disk attached");
                            } else {
                                let r = unsafe { crate::kernel::fat::mkdir(path.as_bytes()) };
                                match r {
                                    0  => self.push_line("directory created"),
                                    -28 => self.push_line("mkdir: disk full"),
                                    _  => self.push_line("mkdir: failed"),
                                }
                            }
                        } else {
                            unsafe {
                                match RAMFS.get() {
                                    Some(fs) => match fs.create_dir(&path) {
                                        Ok(_)    => self.push_line("directory created"),
                                        Err(-17) => self.push_line("mkdir: already exists"),
                                        Err(_)   => self.push_line("mkdir: failed (bad path?)"),
                                    },
                                    None => self.push_line("mkdir: filesystem not ready"),
                                }
                            }
                        }
                    }
                }
            }

            "touch" => {
                match parts.next() {
                    None      => self.push_line("usage: touch <path>"),
                    Some(arg) => {
                        let path = self.resolve_path(arg);
                        unsafe {
                            match RAMFS.get() {
                                Some(fs) => match fs.create_file(&path) {
                                    Ok(_)  => self.push_line("file created"),
                                    Err(_) => self.push_line("touch: failed (bad path?)"),
                                },
                                None => self.push_line("touch: filesystem not ready"),
                            }
                        }
                    }
                }
            }

            "write" => {
                // write <path> <content...>
                let rest = command.strip_prefix("write").unwrap_or("").trim_start();
                let (raw_path, content) = if let Some(sp) = rest.find(' ') {
                    (&rest[..sp], rest[sp + 1..].trim_start())
                } else {
                    (rest, "")
                };
                if raw_path.is_empty() {
                    self.push_line("usage: write <path> <content>");
                } else {
                    let path = self.resolve_path(raw_path);
                    unsafe {
                        match RAMFS.get() {
                            Some(fs) => {
                                let mut data: Vec<u8> = Vec::from(content.as_bytes());
                                data.push(b'\n');
                                match fs.write_file(&path, &data) {
                                    Ok(_)  => self.push_line(&format!(
                                        "wrote {} bytes to {}", data.len(), path)),
                                    Err(_) => self.push_line("write: failed"),
                                }
                            }
                            None => self.push_line("write: filesystem not ready"),
                        }
                    }
                }
            }

            "rm" => {
                match parts.next() {
                    None      => self.push_line("usage: rm <path>"),
                    Some(arg) => {
                        let path = self.resolve_path(arg);
                        unsafe {
                            match RAMFS.get() {
                                Some(fs) => match fs.remove_file(&path) {
                                    Ok(_)    => self.push_line("removed"),
                                    Err(-21) => self.push_line("rm: is a directory"),
                                    Err(-2)  => self.push_line("rm: no such file"),
                                    Err(_)   => self.push_line("rm: failed"),
                                },
                                None => self.push_line("rm: filesystem not ready"),
                            }
                        }
                    }
                }
            }

            "pwd" => self.push_line(&self.cwd.clone()),

            // ── System commands ──────────────────────────────────────────────
            "help" => {
                self.push_line("Filesystem:");
                self.push_line("  ls [path]          - list directory");
                self.push_line("  cd <path>          - change directory");
                self.push_line("  cat <path>         - print file");
                self.push_line("  mkdir <path>       - create directory");
                self.push_line("  touch <path>       - create empty file");
                self.push_line("  write <path> <txt> - write text to file");
                self.push_line("  rm <path>          - remove file");
                self.push_line("  pwd                - print working dir");
                self.push_line("Disk:");
                self.push_line("  ls /disk/          - list FAT16 contents");
                self.push_line("  ls /store/         - list persistent records");
                self.push_line("  cat /store/42      - read record 42");
                self.push_line("  diskinfo           - show all disk/mount info");
                self.push_line("  record list        - list record IDs");
                self.push_line("  record read <id>   - read record");
                self.push_line("  record write <id> <data>");
                self.push_line("  record delete <id>");
                self.push_line("System:");
                self.push_line("  ticks   - timer ticks");
                self.push_line("  uptime  - uptime in ms");
                self.push_line("  pid     - kernel PID");
                self.push_line("  sysinfo - memory & uptime");
                self.push_line("  echo <text>");
                self.push_line("  clear | cls | history | version | about");
                self.push_line("Programs:");
                self.push_line("  run <name>         - spawn (foreground)");
                self.push_line("  run <name> &       - spawn (background)");
                self.push_line("  run                - list programs");
                self.push_line("  sh                 - launch userspace shell");
                self.push_line("  sh &               - shell in background");
                self.push_line("  ps                 - show all tasks");
                self.push_line("  kill <pid>         - terminate a task");
                self.push_line("Navigation:");
                self.push_line("  Left/Right         - move cursor in input");
                self.push_line("  PgUp/PgDn          - scroll history");
                self.push_line("  Ctrl+C             - kill foreground program");
            }

            "clear" | "cls" => self.history.clear(),

            "echo" => {
                let text = command.strip_prefix("echo").unwrap_or("").trim_start();
                self.push_line(text);
            }

            "ticks" => {
                let t = unsafe { timer::get_ticks() };
                self.push_line(&format!("ticks: {}", t));
            }

            "uptime" => {
                let t = unsafe { timer::get_ticks() };
                self.push_line(&format!("uptime: {} ms", t * 10));
            }

            "pid" => {
                let r = unsafe {
                    syscall::handle_syscall(syscall::Syscall::GetPid as u64, 0, 0, 0, 0, 0)
                };
                self.push_line(&format!("pid: {}", r.value));
            }

            "sysinfo" => {
                let info = syscall::snapshot_system_info();
                self.push_line("system:");
                self.push_line(&format!("  uptime:  {} ms",   info.uptime_ms));
                self.push_line(&format!("  total:   {} MB",   info.total_memory / 1024 / 1024));
                self.push_line(&format!("  free:    {} MB",   info.free_memory  / 1024 / 1024));
                self.push_line(&format!("  procs:   {}",      info.process_count));
            }

            "history" => {
                if self.command_history.is_empty() {
                    self.push_line("history: empty");
                } else {
                    let start = self.command_history.len().saturating_sub(10);
                    // Collect first to avoid simultaneous immutable + mutable borrow of self
                    let lines: Vec<String> = self.command_history.iter()
                        .enumerate()
                        .skip(start)
                        .map(|(i, entry)| format!("  {:02}: {}", i + 1, entry))
                        .collect();
                    for line in &lines {
                        self.push_line(line);
                    }
                }
            }

            "version" => {
                self.push_line("OxideOS v0.1.0-dev");
                self.push_line("Built with Rust (no_std), Limine bootloader");
            }

            "about" => {
                self.push_line("OxideOS — a hobby OS written in Rust");
                self.push_line("Features: VFS, RamFS, FAT16, pipes, IPC, GUI");
                self.push_line("Type 'help' for available commands.");
            }

            "shutdown" | "poweroff" => {
                self.push_line("Shutting down...");
                crate::kernel::shutdown::poweroff();
            }

            "reboot" => {
                self.push_line("Rebooting...");
                crate::kernel::shutdown::reboot();
            }

            "run" => {
                let name = parts.next().unwrap_or("");
                if name.is_empty() {
                    self.push_line("usage: run <program> [&]");
                    self.push_line("programs:");
                    for n in crate::kernel::programs::NAMES {
                        self.push_line(&format!("  {}", n));
                    }
                    return;
                }
                // Check for background `&` token
                let background = parts.next().map(|t| t.trim()) == Some("&");
                match crate::kernel::programs::find(name) {
                    None => self.push_line(&format!("run: unknown program '{}'", name)),
                    Some(code) => {
                        match unsafe { crate::kernel::scheduler::spawn(code, name) } {
                            Ok(pid) => {
                                if background {
                                    self.push_line(&format!("spawned '{}' (pid {}) [background]", name, pid));
                                } else {
                                    self.push_line(&format!("spawned '{}' (pid {})", name, pid));
                                    self.enter_passthrough(pid);
                                }
                            }
                            Err(e)  => self.push_line(&format!("run: {}", e)),
                        }
                    }
                }
            }

            "sh" => {
                // `sh &` runs the shell in background (output drains to history)
                let background = parts.next().map(|t| t.trim()) == Some("&");
                match crate::kernel::programs::find("sh") {
                    None => self.push_line("sh: not available"),
                    Some(code) => {
                        match unsafe { crate::kernel::scheduler::spawn(code, "sh") } {
                            Ok(pid) => {
                                if background {
                                    self.push_line(&format!("spawned 'sh' (pid {}) [background]", pid));
                                } else {
                                    self.push_line(&format!("spawned 'sh' (pid {})", pid));
                                    self.enter_passthrough(pid);
                                }
                            }
                            Err(e) => self.push_line(&format!("sh: {}", e)),
                        }
                    }
                }
            }

            "ps" => {
                use crate::kernel::scheduler::TaskState;
                let infos = crate::kernel::scheduler::task_infos();
                let mut any = false;
                for info in &infos {
                    if matches!(info.state, TaskState::Empty) { continue; }
                    let name = core::str::from_utf8(&info.name[..info.name_len]).unwrap_or("?");
                    let state_str = match info.state {
                        TaskState::Empty              => continue,
                        TaskState::Ready              => "ready",
                        TaskState::Running            => "running",
                        TaskState::Sleeping(_)        => "sleeping",
                        TaskState::Waiting(_)         => "waiting",
                        TaskState::WaitingForMsg(_,_) => "ipc-wait",
                        TaskState::Dead(_)            => "dead",
                    };
                    self.push_line(&format!("  [{}] {} ({})", info.pid, name, state_str));
                    any = true;
                }
                if !any { self.push_line("no tasks"); }
            }

            "kill" => {
                match parts.next().and_then(|s| s.parse::<u8>().ok()) {
                    None      => self.push_line("usage: kill <pid>"),
                    Some(pid) => {
                        if unsafe { crate::kernel::scheduler::kill(pid) } {
                            self.push_line(&format!("killed pid {}", pid));
                        } else {
                            self.push_line(&format!("kill: no such task (pid {})", pid));
                        }
                    }
                }
            }

            "diskinfo" => {
                use crate::kernel::ata;
                use crate::kernel::disk_store;
                let labels = ["disk0 (primary master)",  "disk1 (primary slave)",
                              "disk2 (secondary master)","disk3 (secondary slave)"];
                let mounts = ["/disk (FAT16)", "(unmounted)", "/ext2 (ext2)", "(unmounted)"];
                self.push_line("ATA Disks:");
                for i in 0..4usize {
                    if ata::is_present_at(i) {
                        if let Some((secs, _slave, lba48)) = ata::disk_info(i) {
                            let mb = secs / 2048;
                            let mode = if lba48 { "LBA48" } else { "LBA28" };
                            self.push_line(&format!("  {} : {} MB  {}  mount={}",
                                labels[i], mb, mode, mounts[i]));
                        }
                    } else {
                        self.push_line(&format!("  {} : not present", labels[i]));
                    }
                }
                self.push_line("Disk Record Store (/store):");
                for i in [0usize, 2usize] {
                    if disk_store::is_mounted(i) {
                        let mut ids = [0u32; 255];
                        let n = unsafe { disk_store::list_records(i, &mut ids) };
                        self.push_line(&format!("  disk{}: mounted, {} record(s)", i, n));
                    } else {
                        self.push_line(&format!("  disk{}: not mounted", i));
                    }
                }
                self.push_line("Mount Points:");
                self.push_line("  /       ramfs   (volatile)");
                self.push_line("  /disk   fat16   (persistent, disk0)");
                self.push_line("  /ext2   ext2    (read-only, disk2)");
                self.push_line("  /store  oxds    (record store, disk0)");
                self.push_line("  /proc   procfs  (runtime info)");
                self.push_line("  /dev    devfs   (devices)");
            }

            "record" => {
                use crate::kernel::disk_store;
                use crate::kernel::diskfs;
                let sub = parts.next().unwrap_or("");
                match sub {
                    "list" => {
                        if !disk_store::is_mounted(0) {
                            self.push_line("record: disk store not mounted");
                        } else {
                            let mut ids = [0u32; 255];
                            let n = unsafe { disk_store::list_records(0, &mut ids) };
                            if n == 0 {
                                self.push_line("record: no records");
                            } else {
                                self.push_line(&format!("record: {} record(s):", n));
                                for &id in &ids[..n] {
                                    self.push_line(&format!("  id={}", id));
                                }
                            }
                        }
                    }
                    "read" => {
                        let id_str = parts.next().unwrap_or("");
                        match id_str.parse::<u32>() {
                            Err(_) => self.push_line("usage: record read <id>"),
                            Ok(id) => {
                                let mut buf = [0u8; crate::kernel::disk_store::RECORD_DATA_MAX];
                                match unsafe { disk_store::read_record(0, id, &mut buf) } {
                                    None => self.push_line(&format!("record {}: not found", id)),
                                    Some(len) => {
                                        let text = core::str::from_utf8(&buf[..len]).unwrap_or("(binary)");
                                        self.push_line(&format!("record {}: {}", id, text));
                                    }
                                }
                            }
                        }
                    }
                    "write" => {
                        let id_str = parts.next().unwrap_or("");
                        match id_str.parse::<u32>() {
                            Err(_) => self.push_line("usage: record write <id> <data>"),
                            Ok(id) => {
                                let data = parts.collect::<alloc::vec::Vec<_>>().join(" ");
                                if data.is_empty() {
                                    self.push_line("usage: record write <id> <data>");
                                } else if diskfs::write_record(id, data.as_bytes()) {
                                    self.push_line(&format!("record {}: written ({} bytes)", id, data.len()));
                                } else {
                                    self.push_line(&format!("record {}: write failed (disk not mounted or full)", id));
                                }
                            }
                        }
                    }
                    "delete" => {
                        let id_str = parts.next().unwrap_or("");
                        match id_str.parse::<u32>() {
                            Err(_) => self.push_line("usage: record delete <id>"),
                            Ok(id) => {
                                if unsafe { disk_store::delete_record(0, id) } {
                                    self.push_line(&format!("record {}: deleted", id));
                                } else {
                                    self.push_line(&format!("record {}: not found or disk not mounted", id));
                                }
                            }
                        }
                    }
                    _ => {
                        self.push_line("usage: record <list|read|write|delete> [args]");
                        self.push_line("  record list              - list all record IDs");
                        self.push_line("  record read <id>         - read record by ID");
                        self.push_line("  record write <id> <data> - write record");
                        self.push_line("  record delete <id>       - delete record");
                    }
                }
            }

            _ => {
                self.push_line(&format!("unknown: {}  (try 'help')", command));
            }
        }
    }

    // ── Command history ───────────────────────────────────────────────────────

    fn record_command(&mut self, command: &str) {
        if self.command_history.last().map(|s| s.as_str()) != Some(command) {
            self.command_history.push(String::from(command));
            while self.command_history.len() > COMMAND_HISTORY_LIMIT {
                self.command_history.remove(0);
            }
        }
    }

    fn history_up(&mut self) -> bool {
        if self.command_history.is_empty() { return false; }
        let next = match self.history_cursor {
            Some(i) if i > 0 => i - 1,
            Some(0)           => 0,
            None              => self.command_history.len() - 1,
            _                 => return false,
        };
        self.history_cursor = Some(next);
        self.input      = self.command_history[next].clone();
        self.cursor_pos = self.input.len();
        true
    }

    fn history_down(&mut self) -> bool {
        match self.history_cursor {
            Some(i) if i + 1 < self.command_history.len() => {
                self.history_cursor = Some(i + 1);
                self.input      = self.command_history[i + 1].clone();
                self.cursor_pos = self.input.len();
                true
            }
            Some(_) => {
                self.history_cursor = None;
                self.input.clear();
                self.cursor_pos = 0;
                true
            }
            None => false,
        }
    }

    // ── Tab completion ────────────────────────────────────────────────────────

    fn autocomplete(&mut self) -> bool {
        let prefix = String::from(self.input.trim());
        if prefix.is_empty() || prefix.contains(' ') { return false; }

        let matches: Vec<&str> = COMMANDS.iter()
            .copied()
            .filter(|c| c.starts_with(prefix.as_str()))
            .collect();

        let Some(first) = matches.first().copied() else {
            self.push_line("no completions");
            return true;
        };

        if matches.len() > 1 {
            self.push_line("matches:");
            for m in &matches { self.push_line(&format!("  {}", m)); }
            return true;
        }

        self.input = String::from(first);
        if matches!(first, "echo" | "cat" | "cd" | "mkdir" | "touch" | "write" | "rm" | "ls" | "run") {
            self.input.push(' ');
        }
        self.cursor_pos = self.input.len();
        true
    }

    // ── Line buffer ───────────────────────────────────────────────────────────

    /// Strip ANSI escape sequences (e.g. `\x1b[31m`) from `text`.
    fn strip_ansi(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let bytes   = text.as_bytes();
        let mut i   = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                // Skip CSI sequence: ESC [ ... <letter>
                i += 2;
                while i < bytes.len() && !bytes[i].is_ascii_alphabetic() { i += 1; }
                if i < bytes.len() { i += 1; }
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }

    fn push_line(&mut self, raw: &str) {
        const MAX_CHARS: usize = 72;

        // 1. Strip ANSI escape codes
        let cleaned = Self::strip_ansi(raw);

        // 2. Handle carriage return: keep only the last segment after \r
        //    e.g. "Loading...\rDone!" → "Done!"
        let text = if let Some(pos) = cleaned.rfind('\r') {
            &cleaned[pos + 1..]
        } else {
            cleaned.as_str()
        };

        // 3. Auto-scroll to bottom on new output
        self.scroll_offset = 0;

        if text.is_empty() {
            self.history.push(String::new());
            self.trim_history();
            return;
        }

        // 4. Word-wrap at MAX_CHARS
        let bytes = text.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            let end = (start + MAX_CHARS).min(bytes.len());
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
