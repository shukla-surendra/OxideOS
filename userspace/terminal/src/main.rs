//! OxideOS Userspace Terminal
//!
//! Compositor-backed GUI terminal with a program browser, pipe support,
//! command history, and cursor editing.
#![no_std]
#![no_main]

use oxide_rt::{
    exit, fork, waitpid, exec, getchar, sleep_ms, getpid, get_time,
    comp_fill_rect, comp_clear_rect, comp_draw_text, comp_present,
    msgq_create, pipe, dup2, close,
    COMPOSITOR_QID,
};

// ── Layout ────────────────────────────────────────────────────────────────────

const WIN_W: u32 = 556;   // usable content width (window is 560px, 2px border each side)
const WIN_H: u32 = 389;   // approximate; compositor clips to actual window height

const CHAR_W:  u32 = 9;
const LINE_H:  u32 = 16;
const PAD_X:   u32 = 8;
const PAD_Y:   u32 = 6;

const INPUT_H: u32 = 20;
const STATUS_H: u32 = 16;  // status bar at top

// History area: below status bar, above input strip.
const HIST_Y: u32 = PAD_Y + STATUS_H + 4;
const HIST_H: u32 = WIN_H - HIST_Y - INPUT_H - PAD_Y - 4;
const VISIBLE_LINES: usize = (HIST_H / LINE_H) as usize;

const HISTORY_CAP: usize  = 120;
const LINE_CAP:    usize  = 120;
const CMD_HIST_CAP: usize = 32;

// ── Colours ───────────────────────────────────────────────────────────────────

const COL_BG:         u32 = 0xFF0A1220;
const COL_STATUS_BG:  u32 = 0xFF071018;
const COL_STATUS_FG:  u32 = 0xFF3A7ACC;
const COL_INPUT_BG:   u32 = 0xFF0E1A2E;
const COL_CURSOR:     u32 = 0xFF00AAFF;
const COL_PROMPT:     u32 = 0xFF00AAFF;
const COL_DEFAULT:    u32 = 0xFFCCDCEC;
const COL_ERROR:      u32 = 0xFFFF5050;
const COL_INFO:       u32 = 0xFF40C8A0;
const COL_WARN:       u32 = 0xFFFFB030;
const COL_DIM:        u32 = 0xFF506070;
const COL_CMD_ECHO:   u32 = 0xFF6080A0;
const COL_ACCENT:     u32 = 0xFF1A5F9A;
const COL_PROG_NAME:  u32 = 0xFF7FC8FF;
const COL_PROG_DESC:  u32 = 0xFF4A6880;
const COL_SEPARATOR:  u32 = 0xFF1E2840;
const COL_SUCCESS:    u32 = 0xFF40B870;

// ── Known programs (mirrors kernel/src/kernel/programs.rs) ───────────────────

const PROGRAMS: &[(&str, &str)] = &[
    ("hello",      "Print a greeting message"),
    ("hello_rust", "Greeting compiled from Rust/no_std"),
    ("counter",    "Count 1–9 to stdout"),
    ("fib",        "First 15 Fibonacci numbers"),
    ("primes",     "All primes up to 100"),
    ("countdown",  "Countdown 10→1 with 500 ms pauses"),
    ("spinner",    "Animate a spinner for ~3 seconds"),
    ("sysinfo",    "Show system uptime and memory"),
    ("input",      "Echo stdin characters (Ctrl-C to quit)"),
    ("filetest",   "RamFS file create/write/read demo"),
    ("sh",         "Minimal shell with fork/exec/waitpid"),
];

// ── Tiny heap-free string ─────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct FixStr<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> FixStr<N> {
    const fn new() -> Self { Self { buf: [0; N], len: 0 } }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
    fn push(&mut self, b: u8) {
        if self.len < N { self.buf[self.len] = b; self.len += 1; }
    }
    fn push_str(&mut self, s: &str) { for b in s.bytes() { self.push(b); } }
    fn insert(&mut self, pos: usize, b: u8) {
        if self.len >= N || pos > self.len { return; }
        let mut i = self.len;
        while i > pos { self.buf[i] = self.buf[i - 1]; i -= 1; }
        self.buf[pos] = b;
        self.len += 1;
    }
    fn remove(&mut self, pos: usize) {
        if pos >= self.len { return; }
        let mut i = pos;
        while i + 1 < self.len { self.buf[i] = self.buf[i + 1]; i += 1; }
        self.len -= 1;
    }
    fn clear(&mut self) { self.len = 0; }
    fn is_empty(&self) -> bool { self.len == 0 }
    fn starts_with(&self, prefix: &str) -> bool {
        self.as_str().starts_with(prefix)
    }
}

// ── History ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct HistLine {
    text:  FixStr<LINE_CAP>,
    color: u32,
}

impl HistLine {
    const fn empty() -> Self { Self { text: FixStr::new(), color: COL_DEFAULT } }
}

struct History {
    lines:  [HistLine; HISTORY_CAP],
    count:  usize,
    scroll: usize,
}

impl History {
    const fn new() -> Self {
        Self { lines: [HistLine::empty(); HISTORY_CAP], count: 0, scroll: 0 }
    }

    fn push(&mut self, s: &str, color: u32) {
        // Long lines: wrap if necessary
        let bytes = s.as_bytes();
        let max_chars = ((WIN_W - PAD_X * 2) / CHAR_W) as usize;
        if bytes.len() <= max_chars {
            self.push_raw(s, color);
        } else {
            let mut start = 0;
            while start < bytes.len() {
                let end = (start + max_chars).min(bytes.len());
                if let Ok(slice) = core::str::from_utf8(&bytes[start..end]) {
                    self.push_raw(slice, color);
                }
                start = end;
            }
        }
    }

    fn push_raw(&mut self, s: &str, color: u32) {
        let idx = if self.count < HISTORY_CAP {
            self.count
        } else {
            // Shift up (drop oldest line).
            for i in 0..HISTORY_CAP - 1 {
                self.lines[i] = self.lines[i + 1];
            }
            HISTORY_CAP - 1
        };
        self.lines[idx].text.clear();
        self.lines[idx].text.push_str(s);
        self.lines[idx].color = color;
        if self.count < HISTORY_CAP { self.count += 1; }
        if self.scroll > 0 { self.scroll = self.scroll.saturating_sub(1); }
    }

    fn get_visible(&self, row: usize) -> (&str, u32) {
        let view_start = self.count.saturating_sub(VISIBLE_LINES + self.scroll);
        let idx = view_start + self.scroll + row;
        if idx >= self.count { return ("", COL_BG); }
        (&self.lines[idx].text.as_str(), self.lines[idx].color)
    }

    fn scroll_up(&mut self) {
        let max = self.count.saturating_sub(VISIBLE_LINES);
        if self.scroll < max { self.scroll += 4; }
    }
    fn scroll_down(&mut self) { self.scroll = self.scroll.saturating_sub(4); }
}

// ── Terminal state ────────────────────────────────────────────────────────────

struct Terminal {
    history:    History,
    input:      FixStr<256>,
    cursor:     usize,
    cmd_hist:   [FixStr<256>; CMD_HIST_CAP],
    cmd_count:  usize,
    cmd_cursor: Option<usize>,
    dirty:      bool,
}

impl Terminal {
    const fn new() -> Self {
        const EMPTY: FixStr<256> = FixStr::new();
        Self {
            history:    History::new(),
            input:      FixStr::new(),
            cursor:     0,
            cmd_hist:   [EMPTY; CMD_HIST_CAP],
            cmd_count:  0,
            cmd_cursor: None,
            dirty:      true,
        }
    }

    fn log(&mut self, s: &str, color: u32) {
        self.history.push(s, color);
        self.dirty = true;
    }

    fn log_output(&mut self, raw: &str) {
        // Split on newlines and push each segment.
        let mut start = 0;
        for (i, &b) in raw.as_bytes().iter().enumerate() {
            if b == b'\n' {
                self.log(&raw[start..i], COL_DEFAULT);
                start = i + 1;
            }
        }
        if start < raw.len() {
            self.log(&raw[start..], COL_DEFAULT);
        }
    }
}

// ── Compositor drawing helpers ────────────────────────────────────────────────

fn draw_hline(y: u32, color: u32) {
    comp_fill_rect(0, y, WIN_W, 1, color);
}

fn draw_status_bar(pid: u32) {
    comp_fill_rect(0, 0, WIN_W, STATUS_H + PAD_Y + 2, COL_STATUS_BG);
    draw_hline(STATUS_H + PAD_Y + 2, COL_ACCENT);

    comp_draw_text(PAD_X, (PAD_Y + 1) as u32, COL_STATUS_FG,
        " OxideOS Terminal");

    // Right side: pid + uptime
    let ticks = get_time();
    let secs  = ticks / 100;

    let mut rbuf = [0u8; 48];
    let mut rlen = 0usize;

    // "pid:N  up:HH:MM:SS"
    let h = (secs / 3600) % 100;
    let m = (secs / 60)   % 60;
    let s = secs           % 60;

    fn push_u64(buf: &mut [u8], pos: &mut usize, v: u64) {
        let mut tmp = [0u8; 20]; let mut i = tmp.len();
        let mut vv = v;
        if vv == 0 { i -= 1; tmp[i] = b'0'; }
        while vv > 0 { i -= 1; tmp[i] = b'0' + (vv % 10) as u8; vv /= 10; }
        for &b in &tmp[i..] { if *pos < buf.len() { buf[*pos] = b; *pos += 1; } }
    }
    fn push_str(buf: &mut [u8], pos: &mut usize, s: &str) {
        for b in s.bytes() { if *pos < buf.len() { buf[*pos] = b; *pos += 1; } }
    }
    fn push_d2(buf: &mut [u8], pos: &mut usize, v: u64) {
        if *pos < buf.len() { buf[*pos] = b'0' + (v / 10) as u8; *pos += 1; }
        if *pos < buf.len() { buf[*pos] = b'0' + (v % 10) as u8; *pos += 1; }
    }

    push_str(&mut rbuf, &mut rlen, "pid:");
    push_u64(&mut rbuf, &mut rlen, pid as u64);
    push_str(&mut rbuf, &mut rlen, "  up:");
    push_d2(&mut rbuf, &mut rlen, h);
    rbuf[rlen] = b':'; rlen += 1;
    push_d2(&mut rbuf, &mut rlen, m);
    rbuf[rlen] = b':'; rlen += 1;
    push_d2(&mut rbuf, &mut rlen, s);

    let rstr = core::str::from_utf8(&rbuf[..rlen]).unwrap_or("");
    let rx = WIN_W.saturating_sub(rlen as u32 * CHAR_W + PAD_X);
    comp_draw_text(rx, (PAD_Y + 1) as u32, 0xFF2A4A6A, rstr);
}

fn draw_history(term: &Terminal) {
    // Clear history area.
    comp_fill_rect(0, HIST_Y, WIN_W, HIST_H, COL_BG);

    for row in 0..VISIBLE_LINES {
        let y = HIST_Y + row as u32 * LINE_H;
        if y + LINE_H > HIST_Y + HIST_H { break; }
        let (text, color) = term.history.get_visible(row);
        if !text.is_empty() {
            comp_draw_text(PAD_X, y, color, text);
        }
    }

    // Scroll indicator
    if term.history.scroll > 0 {
        comp_draw_text(WIN_W - 3 * CHAR_W - 2, HIST_Y, COL_WARN, "^^^");
    }
}

fn draw_input_area(term: &Terminal) {
    let strip_y = WIN_H - INPUT_H - 2;
    comp_fill_rect(0, strip_y - 1, WIN_W, INPUT_H + 3, COL_INPUT_BG);
    draw_hline(strip_y - 1, COL_ACCENT);

    // Prompt
    let prompt = "$ ";
    comp_draw_text(PAD_X, strip_y + 2, COL_PROMPT, prompt);
    let text_x = PAD_X + prompt.len() as u32 * CHAR_W;

    let input = term.input.as_str();
    let cur   = term.cursor.min(input.len());
    let before = &input[..cur];
    let after  = &input[cur..];

    comp_draw_text(text_x, strip_y + 2, COL_DEFAULT, before);
    let cx = text_x + before.len() as u32 * CHAR_W;

    // Cursor block
    comp_fill_rect(cx, strip_y + 1, CHAR_W, LINE_H - 2, COL_CURSOR);
    if let Some(ch) = after.chars().next() {
        let mut tmp = [0u8; 4];
        let cs = ch.encode_utf8(&mut tmp);
        comp_draw_text(cx, strip_y + 2, COL_BG, cs);
        comp_draw_text(cx + CHAR_W, strip_y + 2, COL_DEFAULT, &after[ch.len_utf8()..]);
    }
}

fn redraw_full(term: &Terminal, pid: u32) {
    comp_fill_rect(0, 0, WIN_W, WIN_H, COL_BG);
    draw_status_bar(pid);
    draw_history(term);
    draw_input_area(term);
    comp_present();
}

// ── Number formatting ─────────────────────────────────────────────────────────

fn fmt_u32_buf(buf: &mut [u8; 16], v: u32) -> &str {
    let mut i = buf.len();
    let mut vv = v;
    if vv == 0 { i -= 1; buf[i] = b'0'; }
    while vv > 0 { i -= 1; buf[i] = b'0' + (vv % 10) as u8; vv /= 10; }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

fn fmt_i64_buf(buf: &mut [u8; 24], v: i64) -> &str {
    if v >= 0 {
        let mut b16 = [0u8; 16];
        let s = fmt_u32_buf(&mut b16, v as u32);
        let off = 8; // use fixed offset region
        let len = s.len();
        buf[off..off+len].copy_from_slice(s.as_bytes());
        return core::str::from_utf8(&buf[off..off+len]).unwrap_or("?");
    }
    // negative: just show the raw i64 as unsigned magnitude with minus
    let mag = (-(v as i128)) as u64;
    let mut tmp = [0u8; 22]; let mut i = tmp.len();
    let mut vv = mag;
    if vv == 0 { i -= 1; tmp[i] = b'0'; }
    while vv > 0 { i -= 1; tmp[i] = b'0' + (vv % 10) as u8; vv /= 10; }
    i -= 1; tmp[i] = b'-';
    let len = tmp.len() - i;
    buf[..len].copy_from_slice(&tmp[i..]);
    core::str::from_utf8(&buf[..len]).unwrap_or("?")
}

// ── Tab completion ────────────────────────────────────────────────────────────

fn tab_complete(term: &mut Terminal) {
    // Copy current input into a local buffer to release borrow on term.
    let mut ibuf = [0u8; 256];
    let ilen = term.input.len.min(255);
    ibuf[..ilen].copy_from_slice(&term.input.buf[..ilen]);
    let input = core::str::from_utf8(&ibuf[..ilen]).unwrap_or("");

    // Only complete the first word.
    if input.contains(' ') { return; }

    const BUILTINS: &[&str] = &["clear","echo","exit","help","pid","programs","ticks"];
    let mut match_buf = [("", ""); 32];
    let mut match_count = 0usize;

    for &b in BUILTINS {
        if b.starts_with(input) && match_count < match_buf.len() {
            match_buf[match_count] = (b, "");
            match_count += 1;
        }
    }
    for &(name, desc) in PROGRAMS {
        if name.starts_with(input) && match_count < match_buf.len() {
            match_buf[match_count] = (name, desc);
            match_count += 1;
        }
    }

    if match_count == 0 { return; }
    if match_count == 1 {
        term.input.clear();
        term.input.push_str(match_buf[0].0);
        term.cursor = term.input.len;
        return;
    }

    // Multiple matches: print them (input borrow already released).
    term.log("", COL_DEFAULT);
    for i in 0..match_count {
        let (name, desc) = match_buf[i];
        let mut line = FixStr::<LINE_CAP>::new();
        line.push_str("  ");
        line.push_str(name);
        if !desc.is_empty() {
            for _ in name.len()..14 { line.push(b' '); }
            line.push_str(desc);
        }
        term.log(line.as_str(), COL_PROG_NAME);
    }
    // Find longest common prefix and extend input to it.
    let first = match_buf[0].0;
    let mut prefix_len = first.len();
    for i in 1..match_count {
        let name = match_buf[i].0;
        prefix_len = prefix_len.min(
            first.bytes().zip(name.bytes()).take_while(|(a, b)| a == b).count()
        );
    }
    if prefix_len > ilen {
        term.input.clear();
        term.input.push_str(&first[..prefix_len]);
        term.cursor = term.input.len;
    }
}

// ── Built-in commands ─────────────────────────────────────────────────────────

fn cmd_help(term: &mut Terminal) {
    term.log("", COL_DEFAULT);
    term.log("  Built-in commands:", COL_INFO);
    term.log("  clear          Clear the terminal", COL_DEFAULT);
    term.log("  echo <text>    Print text to terminal", COL_DEFAULT);
    term.log("  programs       List all available programs", COL_DEFAULT);
    term.log("  pid            Show current process ID", COL_DEFAULT);
    term.log("  ticks          Show kernel tick counter", COL_DEFAULT);
    term.log("  exit           Exit the terminal", COL_DEFAULT);
    term.log("", COL_DEFAULT);
    term.log("  Tips:", COL_INFO);
    term.log("  Tab            Auto-complete command", COL_DIM);
    term.log("  Up / Down      Scroll command history", COL_DIM);
    term.log("  PgUp / PgDn    Scroll output history", COL_DIM);
    term.log("  cmd &          Run in background", COL_DIM);
    term.log("  cmd1 | cmd2    Pipe stdout to stdin", COL_DIM);
    term.log("", COL_DEFAULT);
    term.log("  Type a program name to run it.", COL_DIM);
}

fn cmd_programs(term: &mut Terminal) {
    term.log("", COL_DEFAULT);
    term.log("  Available programs:", COL_INFO);
    term.log("  ──────────────────────────────────────────────", COL_SEPARATOR);
    for &(name, desc) in PROGRAMS {
        let mut line = FixStr::<LINE_CAP>::new();
        line.push_str("  ");
        line.push_str(name);
        for _ in name.len()..12 { line.push(b' '); }
        line.push_str("  ");
        line.push_str(desc);
        term.log(line.as_str(), COL_DEFAULT);
    }
    term.log("  ──────────────────────────────────────────────", COL_SEPARATOR);
    term.log("  Shell: sh  (type 'sh' for an interactive shell)", COL_DIM);
    term.log("", COL_DEFAULT);
}

fn run_builtin(term: &mut Terminal, cmd: &str, args: &str) -> bool {
    match cmd {
        "clear" => {
            term.history.count  = 0;
            term.history.scroll = 0;
            term.dirty = true;
            true
        }
        "help" => { cmd_help(term); true }
        "programs" | "ls" => { cmd_programs(term); true }
        "exit" | "quit" => exit(0),
        "echo" => { term.log(args, COL_DEFAULT); true }
        "pid" => {
            let pid = getpid();
            let mut b = [0u8; 16];
            let mut line = FixStr::<32>::new();
            line.push_str("PID: ");
            line.push_str(fmt_u32_buf(&mut b, pid));
            term.log(line.as_str(), COL_INFO);
            true
        }
        "ticks" => {
            let t = get_time();
            let mut b = [0u8; 24];
            let mut line = FixStr::<48>::new();
            line.push_str("Ticks: ");
            let s = fmt_i64_buf(&mut b, t as i64);
            line.push_str(s);
            term.log(line.as_str(), COL_INFO);
            true
        }
        _ => false,
    }
}

// ── External command execution ────────────────────────────────────────────────

fn run_external(term: &mut Terminal, cmd: &str, background: bool) {
    let child = fork();
    if child < 0 {
        term.log("[error] fork failed", COL_ERROR);
        return;
    }
    if child == 0 {
        let _ = exec(cmd);
        exit(127);
    }
    if background {
        let mut b = [0u8; 16];
        let mut line = FixStr::<64>::new();
        line.push_str("spawned PID ");
        line.push_str(fmt_u32_buf(&mut b, child as u32));
        term.log(line.as_str(), COL_SUCCESS);
        return;
    }
    let code = waitpid(child as u32);
    if code == 127 {
        let mut line = FixStr::<LINE_CAP>::new();
        line.push_str("[error] '");
        line.push_str(cmd);
        line.push_str("' not found  (try 'programs' to list)");
        term.log(line.as_str(), COL_ERROR);
    } else if code != 0 {
        let mut b = [0u8; 24];
        let mut line = FixStr::<64>::new();
        line.push_str("exited ");
        line.push_str(fmt_i64_buf(&mut b, code));
        term.log(line.as_str(), COL_DIM);
    }
}

// ── Pipe execution ────────────────────────────────────────────────────────────

fn run_pipe(term: &mut Terminal, left_cmd: &str, right_cmd: &str) {
    let mut r: i32 = -1;
    let mut w: i32 = -1;
    if pipe(&mut r, &mut w) < 0 {
        term.log("[error] pipe() failed", COL_ERROR);
        return;
    }
    let left_pid = fork();
    if left_pid < 0 {
        close(r); close(w);
        term.log("[error] fork failed (left)", COL_ERROR);
        return;
    }
    if left_pid == 0 {
        close(r); dup2(w, 1); close(w);
        let _ = exec(left_cmd);
        exit(127);
    }
    let right_pid = fork();
    if right_pid < 0 {
        close(r); close(w);
        term.log("[error] fork failed (right)", COL_ERROR);
        waitpid(left_pid as u32);
        return;
    }
    if right_pid == 0 {
        close(w); dup2(r, 0); close(r);
        let _ = exec(right_cmd);
        exit(127);
    }
    close(r); close(w);
    waitpid(left_pid as u32);
    waitpid(right_pid as u32);
}

// ── Command submission ────────────────────────────────────────────────────────

fn submit(term: &mut Terminal) {
    // Copy into a local buffer to release the borrow on term.input.
    let mut local = [0u8; 256];
    let len = term.input.len.min(255);
    local[..len].copy_from_slice(&term.input.buf[..len]);
    let raw   = core::str::from_utf8(&local[..len]).unwrap_or("");
    let trimmed = raw.trim();

    term.input.clear();
    term.cursor    = 0;
    term.cmd_cursor = None;

    if trimmed.is_empty() { return; }

    // Echo
    let mut echo = FixStr::<LINE_CAP>::new();
    echo.push_str("> ");
    // Truncate echo if too long.
    let tlen = trimmed.len().min(LINE_CAP - 3);
    for b in trimmed.as_bytes()[..tlen].iter() { echo.push(*b); }
    term.log(echo.as_str(), COL_CMD_ECHO);

    // Save to command history.
    if term.cmd_count < CMD_HIST_CAP {
        term.cmd_hist[term.cmd_count].clear();
        term.cmd_hist[term.cmd_count].push_str(trimmed);
        term.cmd_count += 1;
    } else {
        for i in 0..CMD_HIST_CAP - 1 { term.cmd_hist[i] = term.cmd_hist[i + 1]; }
        term.cmd_hist[CMD_HIST_CAP - 1].clear();
        term.cmd_hist[CMD_HIST_CAP - 1].push_str(trimmed);
    }

    // Background?
    let (run_str, background) = if trimmed.ends_with(" &") {
        (&trimmed[..trimmed.len() - 2], true)
    } else {
        (trimmed, false)
    };

    // Copy run_str locally before calling mutable methods on term.
    let mut cmd_buf = [0u8; 256];
    let cl = run_str.len().min(255);
    cmd_buf[..cl].copy_from_slice(&run_str.as_bytes()[..cl]);
    let cmd_owned = core::str::from_utf8(&cmd_buf[..cl]).unwrap_or("");

    // Pipe?
    if let Some(pipe_pos) = cmd_owned.as_bytes().iter().position(|&b| b == b'|') {
        let left  = cmd_owned[..pipe_pos].trim();
        let right = cmd_owned[pipe_pos + 1..].trim();
        let mut lb = [0u8; 128]; let ll = left.len().min(127);
        let mut rb = [0u8; 128]; let rl = right.len().min(127);
        lb[..ll].copy_from_slice(&left.as_bytes()[..ll]);
        rb[..rl].copy_from_slice(&right.as_bytes()[..rl]);
        let ls = core::str::from_utf8(&lb[..ll]).unwrap_or("");
        let rs = core::str::from_utf8(&rb[..rl]).unwrap_or("");
        run_pipe(term, ls, rs);
        return;
    }

    // Split first word.
    let (cmd, args) = if let Some(i) = cmd_owned.bytes().position(|b| b == b' ') {
        (cmd_owned[..i].trim(), cmd_owned[i + 1..].trim())
    } else {
        (cmd_owned.trim(), "")
    };

    if !run_builtin(term, cmd, args) {
        run_external(term, cmd_owned, background);
    }
}

// ── History navigation ────────────────────────────────────────────────────────

fn history_up(term: &mut Terminal) {
    if term.cmd_count == 0 { return; }
    let next = match term.cmd_cursor {
        None    => term.cmd_count - 1,
        Some(0) => 0,
        Some(i) => i - 1,
    };
    term.cmd_cursor = Some(next);
    term.input.clear();
    term.input.push_str(term.cmd_hist[next].as_str());
    term.cursor = term.input.len;
}

fn history_down(term: &mut Terminal) {
    match term.cmd_cursor {
        None => {}
        Some(i) if i + 1 >= term.cmd_count => {
            term.cmd_cursor = None;
            term.input.clear();
            term.cursor = 0;
        }
        Some(i) => {
            term.cmd_cursor = Some(i + 1);
            term.input.clear();
            term.input.push_str(term.cmd_hist[i + 1].as_str());
            term.cursor = term.input.len;
        }
    }
}

// ── Escape-sequence handling ──────────────────────────────────────────────────

fn blocking_getchar() -> u8 {
    loop {
        if let Some(c) = getchar() { return c; }
        sleep_ms(5);
    }
}

fn handle_escape(term: &mut Terminal) {
    if blocking_getchar() != b'[' { return; }
    match blocking_getchar() {
        b'A' => history_up(term),
        b'B' => history_down(term),
        b'C' => { let n = term.input.len; if term.cursor < n { term.cursor += 1; } }
        b'D' => { if term.cursor > 0 { term.cursor -= 1; } }
        b'5' => { let _ = blocking_getchar(); term.history.scroll_up();   term.dirty = true; }
        b'6' => { let _ = blocking_getchar(); term.history.scroll_down(); term.dirty = true; }
        _    => {}
    }
}

// ── Startup banner ────────────────────────────────────────────────────────────

fn print_banner(term: &mut Terminal) {
    term.log("", COL_DEFAULT);
    term.log("  ___          _    _       ___  ___", 0xFF007ACC);
    term.log(" / _ \\ __ _(_)__| | ___ / _ \\/ __|", 0xFF007ACC);
    term.log("| (_) | V / / _` |/ -_) (_) \\__ \\", 0xFF0060AA);
    term.log(" \\___/ \\_/|_\\__,_|\\___|\\ ___/|___/", 0xFF004888);
    term.log("", COL_DEFAULT);
    term.log("  Userspace Terminal  —  OxideOS v0.1", COL_INFO);
    term.log("  Type 'help' for commands, 'programs' to list apps", COL_DIM);
    term.log("  Tab-completion and arrow-key history supported", COL_DIM);
    term.log("", COL_DEFAULT);
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    // Connect to compositor (queue was created by kernel at boot).
    msgq_create(COMPOSITOR_QID);

    let my_pid = getpid();
    let mut term = Terminal::new();

    print_banner(&mut term);
    redraw_full(&term, my_pid);
    term.dirty = false;

    let mut frame_tick: u64 = 0;

    loop {
        let Some(c) = getchar() else {
            // Update status bar every ~100 frames (~1 second) even when idle.
            frame_tick += 1;
            if frame_tick % 100 == 0 {
                draw_status_bar(my_pid);
                comp_present();
            }
            if term.dirty {
                redraw_full(&term, my_pid);
                term.dirty = false;
            }
            sleep_ms(10);
            continue;
        };

        match c {
            b'\n' | b'\r' => {
                submit(&mut term);
                redraw_full(&term, my_pid);
                term.dirty = false;
            }
            b'\t' => {
                tab_complete(&mut term);
                if term.dirty {
                    redraw_full(&term, my_pid);
                    term.dirty = false;
                } else {
                    draw_input_area(&term);
                    comp_present();
                }
            }
            // Backspace / DEL
            8 | 127 => {
                if term.cursor > 0 {
                    term.cursor -= 1;
                    term.input.remove(term.cursor);
                    draw_input_area(&term);
                    comp_present();
                }
            }
            // Ctrl-C — cancel current input
            3 => {
                term.input.clear();
                term.cursor = 0;
                term.cmd_cursor = None;
                draw_input_area(&term);
                comp_present();
            }
            // Ctrl-L — clear screen
            12 => {
                term.history.count  = 0;
                term.history.scroll = 0;
                redraw_full(&term, my_pid);
            }
            // ESC sequence (arrow keys)
            0x1B => {
                handle_escape(&mut term);
                if term.dirty {
                    redraw_full(&term, my_pid);
                    term.dirty = false;
                } else {
                    draw_input_area(&term);
                    comp_present();
                }
            }
            // Printable ASCII
            c if c >= 32 && c < 127 => {
                term.input.insert(term.cursor, c);
                term.cursor += 1;
                term.cmd_cursor = None;
                draw_input_area(&term);
                comp_present();
            }
            _ => {}
        }
    }
}
