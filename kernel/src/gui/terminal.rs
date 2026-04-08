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

// ── Tab-completion word list ───────────────────────────────────────────────
const COMMANDS: &[&str] = &[
    "about", "cat", "clear", "cls", "echo",
    "help", "history", "ls", "mkdir", "pid",
    "ps", "pwd", "rm", "run", "sysinfo", "ticks", "touch",
    "uptime", "version", "write",
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
        keyboard::ArrowKey::Up    => EVENT_ARROW_UP,
        keyboard::ArrowKey::Down  => EVENT_ARROW_DOWN,
        keyboard::ArrowKey::Left  => EVENT_ARROW_LEFT,
        keyboard::ArrowKey::Right => EVENT_ARROW_RIGHT,
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
    window_id:       usize,
    history:         Vec<String>,
    input:           String,
    command_history: Vec<String>,
    history_cursor:  Option<usize>,
}

impl TerminalApp {
    pub fn new(window_id: usize) -> Self {
        let mut t = Self {
            window_id,
            history:         Vec::new(),
            input:           String::new(),
            command_history: Vec::new(),
            history_cursor:  None,
        };
        t.print_banner();
        t
    }

    pub fn window_id(&self) -> usize { self.window_id }

    /// Called by the main loop when the background task exits.
    /// Drains captured stdout and shows the exit code.
    pub fn on_task_exit(&mut self, exit_code: i64) {
        unsafe {
            crate::kernel::user_mode::output_drain_lines(|line| {
                self.push_line(line);
            });
        }
        self.push_line(&format!("exited with code {}", exit_code));
    }

    pub fn process_pending_input(&mut self, focused: bool) -> bool {
        let mut changed = false;
        while let Some(event) = dequeue_event() {
            if focused { changed |= self.handle_event(event); }
        }
        changed
    }

    // ── Drawing ─────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager) {
        if !wm.is_window_visible(self.window_id) { return; }
        let Some(window) = wm.get_window(self.window_id) else { return; };

        let is_focused    = wm.get_focused() == Some(self.window_id);
        let content_x     = window.x + CONTENT_PADDING_X;
        let content_y     = window.y + 30 + CONTENT_PADDING_Y;
        let content_width = window.width.saturating_sub(CONTENT_PADDING_X * 2);
        let content_height= window.height.saturating_sub(30 + CONTENT_PADDING_Y * 2);
        let output_height = content_height.saturating_sub(INPUT_HEIGHT + 6);
        let input_y       = content_y + output_height + 6;
        let max_chars     = (content_width / CHAR_WIDTH).max(4) as usize;
        let visible_lines = (output_height / LINE_HEIGHT).max(1) as usize;

        // Output area — dark background with subtle top border
        graphics.fill_rect(content_x, content_y, content_width, output_height, 0xFF0A1018);
        graphics.fill_rect(content_x, content_y, content_width, 1, 0xFF1A5F9A);
        graphics.draw_rect(content_x, content_y, content_width, output_height, 0xFF1E2840, 1);

        // Input area
        graphics.fill_rect(content_x, input_y, content_width, INPUT_HEIGHT, 0xFF0D1520);
        graphics.fill_rect(content_x, input_y, content_width, 1,
                           if is_focused { 0xFF007ACC } else { 0xFF1E2840 });
        graphics.draw_rect(content_x, input_y, content_width, INPUT_HEIGHT,
                           if is_focused { 0xFF1A5F9A } else { 0xFF151E2E }, 1);

        // History lines — colour-coded
        let start_line = self.history.len().saturating_sub(visible_lines);
        for (i, line) in self.history.iter().skip(start_line).enumerate() {
            let color = line_color(line);
            fonts::draw_string(graphics, content_x + 6,
                               content_y + 2 + i as u64 * LINE_HEIGHT,
                               line, color);
        }

        // Prompt: coloured ">" then input text
        let prompt_sym = ">";
        let input_display: alloc::string::String = if self.input.len() > max_chars.saturating_sub(3) {
            alloc::string::String::from(&self.input[self.input.len().saturating_sub(max_chars.saturating_sub(3))..])
        } else {
            self.input.clone()
        };
        fonts::draw_string(graphics, content_x + 6, input_y + 8,
                           prompt_sym, 0xFF00D060);           // bright green >
        fonts::draw_string(graphics, content_x + 6 + CHAR_WIDTH * 2, input_y + 8,
                           &input_display, 0xFFD8E8FF);       // light blue input text

        if is_focused {
            let cx = content_x + 6 + CHAR_WIDTH * 2 + input_display.len() as u64 * CHAR_WIDTH;
            // Blinking-style cursor block
            graphics.fill_rect(cx, input_y + 5, 2, LINE_HEIGHT - 2, 0xFF00AAFF);
        }
    }

    // ── Input handling ───────────────────────────────────────────────────────

    fn handle_event(&mut self, event: u16) -> bool {
        match event {
            EVENT_ARROW_UP              => self.history_up(),
            EVENT_ARROW_DOWN            => self.history_down(),
            EVENT_ARROW_LEFT |
            EVENT_ARROW_RIGHT           => false,
            _                           => self.handle_key(event as u8),
        }
    }

    fn handle_key(&mut self, ch: u8) -> bool {
        match ch {
            8 => { self.history_cursor = None; self.input.pop(); true }
            b'\n' | b'\r' => { self.submit_command(); true }
            b'\t'         => self.autocomplete(),
            32..=126      => {
                self.history_cursor = None;
                self.input.push(ch as char);
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
            "ls" => {
                let path = parts.next().unwrap_or("/");
                unsafe {
                    match RAMFS.get() {
                        Some(fs) => match fs.list_dir(path) {
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

            "cat" => {
                match parts.next() {
                    None       => self.push_line("usage: cat <path>"),
                    Some(path) => unsafe {
                        match RAMFS.get() {
                            Some(fs) => match fs.read_file(path) {
                                Some(data) => {
                                    if data.is_empty() {
                                        self.push_line("(empty file)");
                                    } else {
                                        match core::str::from_utf8(data) {
                                            Ok(text) => {
                                                for line in text.lines() {
                                                    self.push_line(line);
                                                }
                                            }
                                            Err(_) => {
                                                self.push_line(&format!(
                                                    "(binary, {} bytes)", data.len()));
                                            }
                                        }
                                    }
                                }
                                None => self.push_line("cat: no such file"),
                            },
                            None => self.push_line("cat: filesystem not ready"),
                        }
                    },
                }
            }

            "mkdir" => {
                match parts.next() {
                    None       => self.push_line("usage: mkdir <path>"),
                    Some(path) => unsafe {
                        match RAMFS.get() {
                            Some(fs) => match fs.create_dir(path) {
                                Ok(_)    => self.push_line("directory created"),
                                Err(-17) => self.push_line("mkdir: already exists"),
                                Err(_)   => self.push_line("mkdir: failed (bad path?)"),
                            },
                            None => self.push_line("mkdir: filesystem not ready"),
                        }
                    },
                }
            }

            "touch" => {
                match parts.next() {
                    None       => self.push_line("usage: touch <path>"),
                    Some(path) => unsafe {
                        match RAMFS.get() {
                            Some(fs) => match fs.create_file(path) {
                                Ok(_)  => self.push_line("file created"),
                                Err(_) => self.push_line("touch: failed (bad path?)"),
                            },
                            None => self.push_line("touch: filesystem not ready"),
                        }
                    },
                }
            }

            "write" => {
                // write <path> <content...>
                let rest = command.strip_prefix("write").unwrap_or("").trim_start();
                let (path, content) = if let Some(sp) = rest.find(' ') {
                    (&rest[..sp], rest[sp + 1..].trim_start())
                } else {
                    (rest, "")
                };
                if path.is_empty() {
                    self.push_line("usage: write <path> <content>");
                } else {
                    unsafe {
                        match RAMFS.get() {
                            Some(fs) => {
                                let mut data: Vec<u8> = Vec::from(content.as_bytes());
                                data.push(b'\n');
                                match fs.write_file(path, &data) {
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
                    None       => self.push_line("usage: rm <path>"),
                    Some(path) => unsafe {
                        match RAMFS.get() {
                            Some(fs) => match fs.remove_file(path) {
                                Ok(_)    => self.push_line("removed"),
                                Err(-21) => self.push_line("rm: is a directory"),
                                Err(-2)  => self.push_line("rm: no such file"),
                                Err(_)   => self.push_line("rm: failed"),
                            },
                            None => self.push_line("rm: filesystem not ready"),
                        }
                    },
                }
            }

            "pwd" => self.push_line("/"),

            // ── System commands ──────────────────────────────────────────────
            "help" => {
                self.push_line("Filesystem:");
                self.push_line("  ls [path]          - list directory");
                self.push_line("  cat <path>         - print file");
                self.push_line("  mkdir <path>       - create directory");
                self.push_line("  touch <path>       - create empty file");
                self.push_line("  write <path> <txt> - write text to file");
                self.push_line("  rm <path>          - remove file");
                self.push_line("  pwd                - print working dir");
                self.push_line("System:");
                self.push_line("  ticks   - timer ticks");
                self.push_line("  uptime  - uptime in ms");
                self.push_line("  pid     - kernel PID");
                self.push_line("  sysinfo - memory & uptime");
                self.push_line("  echo <text>");
                self.push_line("  clear | cls | history | version | about");
                self.push_line("Programs:");
                self.push_line("  run <name>         - spawn user program");
                self.push_line("  run                - list programs");
                self.push_line("  ps                 - show running task");
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
                self.push_line("OxideOS kernel prototype");
                self.push_line("GUI terminal revision 3 (with RamFS)");
            }

            "about" => {
                self.push_line("OxideOS GUI terminal");
                self.push_line("Features: RamFS, history, tab-complete");
                self.push_line("Filesystem: in-memory RamFS");
                self.push_line("Next: process scheduler, ELF loader");
            }

            "run" => {
                let name = parts.next().unwrap_or("");
                if name.is_empty() {
                    self.push_line("usage: run <program>");
                    self.push_line("programs:");
                    for n in crate::kernel::programs::NAMES {
                        self.push_line(&format!("  {}", n));
                    }
                    return;
                }
                match crate::kernel::programs::find(name) {
                    None => self.push_line(&format!("run: unknown program '{}'", name)),
                    Some(code) => {
                        unsafe {
                            match crate::kernel::scheduler::spawn(code, name) {
                                Ok(_)  => self.push_line(&format!("spawned '{}'", name)),
                                Err(e) => self.push_line(&format!("run: {}", e)),
                            }
                        }
                    }
                }
            }

            "ps" => {
                let task = unsafe { &*(&raw const crate::kernel::scheduler::SCHED.task) };
                use crate::kernel::scheduler::TaskState;
                match task.state {
                    TaskState::Empty        => self.push_line("no tasks"),
                    TaskState::Ready        => self.push_line(&format!("[1] {} (ready)",   task.name_str())),
                    TaskState::Running      => self.push_line(&format!("[1] {} (running)", task.name_str())),
                    TaskState::Sleeping(t)  => self.push_line(&format!("[1] {} (sleeping until tick {})", task.name_str(), t)),
                    TaskState::Dead(code)   => self.push_line(&format!("[1] {} (exited {})", task.name_str(), code)),
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
        self.input = self.command_history[next].clone();
        true
    }

    fn history_down(&mut self) -> bool {
        match self.history_cursor {
            Some(i) if i + 1 < self.command_history.len() => {
                self.history_cursor = Some(i + 1);
                self.input = self.command_history[i + 1].clone();
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
        if matches!(first, "echo" | "cat" | "mkdir" | "touch" | "write" | "rm" | "ls" | "run") {
            self.input.push(' ');
        }
        true
    }

    // ── Line buffer ───────────────────────────────────────────────────────────

    fn push_line(&mut self, text: &str) {
        const MAX_CHARS: usize = 72;
        if text.is_empty() {
            self.history.push(String::new());
            self.trim_history();
            return;
        }
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
