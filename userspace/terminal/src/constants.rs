//! Compile-time layout, color, and program-list constants for the terminal.

// ── Window layout ─────────────────────────────────────────────────────────────
pub const WIN_W: u32 = 556;
pub const WIN_H: u32 = 389;

pub const CHAR_W:  u32 = 9;
pub const LINE_H:  u32 = 16;
pub const PAD_X:   u32 = 8;
pub const PAD_Y:   u32 = 6;
pub const INPUT_H: u32 = 20;
pub const STATUS_H: u32 = 16;

pub const HIST_Y: u32 = PAD_Y + STATUS_H + 4;
pub const HIST_H: u32 = WIN_H - HIST_Y - INPUT_H - PAD_Y - 4;
pub const VISIBLE_LINES: usize = (HIST_H / LINE_H) as usize;

pub const HISTORY_CAP:  usize = 120;
pub const LINE_CAP:     usize = 120;
pub const CMD_HIST_CAP: usize = 32;

// ── Colors ────────────────────────────────────────────────────────────────────
pub const COL_BG:        u32 = 0xFF0C0C0C;
pub const COL_STATUS_BG: u32 = 0xFF1A1A1A;
pub const COL_STATUS_FG: u32 = 0xFF4EC9B0;
pub const COL_INPUT_BG:  u32 = 0xFF121212;
pub const COL_CURSOR:    u32 = 0xFF4EC9B0;
pub const COL_PROMPT:    u32 = 0xFF569CD6;
pub const COL_DEFAULT:   u32 = 0xFFCCCCCC;
pub const COL_ERROR:     u32 = 0xFFF14C4C;
pub const COL_INFO:      u32 = 0xFF4EC9B0;
pub const COL_WARN:      u32 = 0xFFDDBB00;
pub const COL_DIM:       u32 = 0xFF555555;
pub const COL_CMD_ECHO:  u32 = 0xFF608060;
pub const COL_ACCENT:    u32 = 0xFF2A2A2A;
pub const COL_PROG_NAME: u32 = 0xFF7FC8FF;
pub const COL_SEPARATOR: u32 = 0xFF1E2840;
pub const COL_SUCCESS:   u32 = 0xFF40B870;

// ── Known programs (mirrors kernel/src/kernel/programs.rs) ───────────────────
pub const PROGRAMS: &[(&str, &str)] = &[
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
    ("edit",       "Full-screen text editor (Ctrl+S save, Ctrl+Q quit)"),
    ("ls",         "List directory contents"),
    ("cat",        "Print file contents"),
    ("ps",         "Show running processes"),
    ("wget",       "Minimal HTTP GET client"),
    ("browser",    "HTTP web browser with GUI and link navigation"),
];
