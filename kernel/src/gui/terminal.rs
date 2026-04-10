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
    "about", "cat", "cd", "clear", "cls", "echo",
    "help", "history", "kill", "ls", "mkdir", "pid",
    "ps", "pwd", "reboot", "rm", "run", "sh", "shutdown", "sysinfo",
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
        0xFF40C8A0  // teal — informational
    } else if line.starts_with("[error]") || line.starts_with("error:") {
        0xFFFF5050  // red — errors
    } else if line.starts_with("[warn]") {
        0xFFFFB030  // amber — warnings
    } else if line.starts_with('>') {
        0xFF8090B0  // dim slate — echoed commands
    } else if line.starts_with("  ") {
        0xFF607898  // dim — directory listings / indented
    } else if line.starts_with("exited") {
        0xFF60A070  // green — exit messages
    } else if line.starts_with("spawned") {
        0xFF40C870  // bright green — spawn confirmation
    } else if line.starts_with(' ') && line.contains('/') {
        0xFF5090C0  // blue — paths
    } else if line.starts_with("  ___") || line.starts_with(" / _")
           || line.starts_with("| |")  || line.starts_with(" \\___") {
        0xFF007ACC  // brand blue — ASCII art banner
    } else {
        0xFFCCDCEC  // default: light blue-white
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
    /// Lines scrolled back from the bottom (0 = follow tail).
    scroll_offset:    usize,
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
            scroll_offset:    0,
        };
        t.print_banner();
        t
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
        self.push_line(&format!("[pid {}] exited with code {}", pid, exit_code));
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
        self.passthrough_mode = true;
        self.fg_pid           = Some(pid);
    }

    fn exit_passthrough(&mut self) {
        self.passthrough_mode = false;
        self.fg_pid           = None;
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
                    // All other keystrokes already in stdin ring via keyboard.rs
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
    // The terminal looks like a real shell: a single full-area text surface.
    // History lines fill from top; the prompt + input appear as the last line.
    // No separate "chat box" — everything is inline text on a dark background.

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager) {
        if !wm.is_window_visible(self.window_id) { return; }
        let Some(window) = wm.get_window(self.window_id) else { return; };

        let is_focused = wm.get_focused() == Some(self.window_id);

        // Content area (below title bar, small inset).
        let cx = window.x + 4;
        let cy = window.y + 31;                             // title bar height = 30 + 1px line
        let cw = window.width.saturating_sub(8);
        let ch = window.height.saturating_sub(31 + 4);

        // Full dark terminal background — no borders or boxes.
        graphics.fill_rect(cx, cy, cw, ch, 0xFF0C1014);

        // Thin focus indicator on the left edge (like a cursor line in some terminals).
        if is_focused {
            graphics.fill_rect(cx, cy, 2, ch, 0xFF007ACC);
        }

        let text_x = cx + 6;
        let max_cols = (cw.saturating_sub(12) / CHAR_WIDTH).max(4) as usize;

        // The bottom line is reserved for the prompt+input (or running indicator).
        // All lines above it show history.
        let total_lines   = (ch / LINE_HEIGHT).max(2) as usize;
        let history_lines = total_lines - 1;   // one line for the input row

        // ── History ────────────────────────────────────────────────────────
        let end_idx   = self.history.len().saturating_sub(self.scroll_offset);
        let start_idx = end_idx.saturating_sub(history_lines);

        // Scroll-back indicator (top line when scrolled)
        if self.scroll_offset > 0 {
            let above = self.history.len()
                .saturating_sub(history_lines + self.scroll_offset);
            let msg = format!("-- {} more lines above -- (PgDn to scroll down)",
                              above + self.scroll_offset);
            let row_y = cy + 2;
            graphics.fill_rect(cx, row_y, cw, LINE_HEIGHT, 0xFF141C20);
            fonts::draw_string(graphics, text_x, row_y + 2, &msg, 0xFF607080);
            // Show one fewer history line to make room.
            let hist_start = end_idx.saturating_sub(history_lines - 1);
            for (row, line) in self.history.iter()
                .skip(hist_start).take(history_lines - 1).enumerate()
            {
                let y = cy + 2 + (row as u64 + 1) * LINE_HEIGHT;
                fonts::draw_string(graphics, text_x, y, line, line_color(line));
            }
        } else {
            for (row, line) in self.history.iter()
                .skip(start_idx).take(history_lines).enumerate()
            {
                let y = cy + 2 + row as u64 * LINE_HEIGHT;
                fonts::draw_string(graphics, text_x, y, line, line_color(line));
            }
        }

        // ── Prompt / input line (bottom row) ───────────────────────────────
        let input_row_y = cy + ch - LINE_HEIGHT - 2;

        // Subtle highlight on the active input line.
        graphics.fill_rect(cx, input_row_y, cw, LINE_HEIGHT + 2, 0xFF101820);

        if self.passthrough_mode {
            // Running a foreground program — show amber status.
            let indicator = match self.fg_pid {
                Some(pid) => format!("[pid {}] running  (Ctrl+C to kill)", pid),
                None      => String::from("[running...]"),
            };
            fonts::draw_string(graphics, text_x, input_row_y + 2,
                               &indicator, 0xFFFFAA00);
        } else {
            // Prompt: "oxide:~$ " — green like bash.
            let prompt     = "oxide:~$ ";
            let prompt_len = prompt.len() as u64;
            fonts::draw_string(graphics, text_x, input_row_y + 2,
                               prompt, 0xFF33DD66);   // bright green

            // Input text — scrolls to keep cursor visible.
            let avail_cols  = max_cols.saturating_sub(prompt.len());
            let win_start = if self.cursor_pos > avail_cols {
                self.cursor_pos - avail_cols
            } else {
                0
            };
            let win_start = {
                let mut s = win_start;
                while s > 0 && !self.input.is_char_boundary(s) { s -= 1; }
                s
            };
            let visible_slice = &self.input[win_start..];
            let display: String = if visible_slice.len() > avail_cols {
                String::from(&visible_slice[..avail_cols])
            } else {
                String::from(visible_slice)
            };

            let input_text_x = text_x + prompt_len * CHAR_WIDTH;
            fonts::draw_string(graphics, input_text_x, input_row_y + 2,
                               &display, 0xFFDDEEFF);  // near-white text

            // Block cursor: draw a filled rectangle behind the character at cursor.
            if is_focused {
                let chars_before  = self.cursor_pos.saturating_sub(win_start);
                let cursor_x      = input_text_x + chars_before as u64 * CHAR_WIDTH;
                // Filled block cursor.
                graphics.fill_rect(cursor_x, input_row_y + 1, CHAR_WIDTH, LINE_HEIGHT, 0xFF007ACC);
                // Character on top of cursor (in background colour so it's readable).
                if let Some(ch) = self.input[self.cursor_pos..].chars().next() {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    fonts::draw_string(graphics, cursor_x, input_row_y + 2, s, 0xFF0C1014);
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
        self.push_line("  ___          _    _       ___  ____");
        self.push_line(" / _ \\ __  __ (_)  | |     / _ \\/ ___|");
        self.push_line("| | | |\\ \\/ / | |  | |  _ | | | \\___ \\");
        self.push_line("| |_| | >  <  | |  | |_| || |_| |___) |");
        self.push_line(" \\___/ /_/\\_\\ |_|  |_____| \\___/|____/");
        self.push_line("");
        self.push_line("[info] Type 'help' for commands. Tab completes.");
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
                    // /disk itself — treat as a virtual directory rooted on FAT
                    if crate::kernel::ata::is_present() {
                        self.cwd = String::from("/disk/");
                    } else {
                        self.push_line("cd: /disk: no disk attached");
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
                    // List FAT root directory
                    if !crate::kernel::ata::is_present() {
                        self.push_line("ls: no disk attached");
                    } else {
                        let entries = unsafe { crate::kernel::fat::list_root() };
                        if entries.is_empty() {
                            self.push_line("(empty disk directory)");
                        } else {
                            for (name, is_dir) in &entries {
                                let suffix = if *is_dir { "/" } else { "" };
                                self.push_line(&format!("  {}{}", name, suffix));
                            }
                            self.push_line(&format!("  ({} entries)", entries.len()));
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
                self.push_line("Disk (FAT16):");
                self.push_line("  ls /disk/          - list disk contents");
                self.push_line("  cd /disk/          - enter disk directory");
                self.push_line("  cat /disk/FILE     - read file from disk");
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
