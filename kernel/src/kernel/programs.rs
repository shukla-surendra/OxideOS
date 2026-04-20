//! Built-in user programs embedded as flat binaries.
//!
//! Each binary is a position-independent flat binary assembled by NASM
//! (`nasm -f bin`) and linked at org 0x400000 (USER_CODE_ADDR).
//! They use the `int 0x80` syscall ABI defined in syscall_core.rs.

/// "Hello, World" — prints a greeting and exits.
pub static HELLO: &[u8] =
    include_bytes!("../../../userspace/bin/hello.bin");

/// Counter — prints the digits 1–9 one per line.
pub static COUNTER: &[u8] =
    include_bytes!("../../../userspace/bin/counter.bin");

/// Sysinfo — calls GetSystemInfo and prints uptime + memory.
pub static SYSINFO: &[u8] =
    include_bytes!("../../../userspace/bin/sysinfo.bin");

/// Input — echoes stdin characters, exits on Ctrl+C.
pub static INPUT: &[u8] =
    include_bytes!("../../../userspace/bin/input.bin");

/// Fib — prints the first 15 Fibonacci numbers.
pub static FIB: &[u8] =
    include_bytes!("../../../userspace/bin/fib.bin");

/// Primes — prints all primes up to 100.
pub static PRIMES: &[u8] =
    include_bytes!("../../../userspace/bin/primes.bin");

/// Countdown — counts down from 10 to 1 with 500 ms delays, then "Liftoff!".
pub static COUNTDOWN: &[u8] =
    include_bytes!("../../../userspace/bin/countdown.bin");

/// Spinner — animates a spinning cursor for ~3 seconds.
pub static SPINNER: &[u8] =
    include_bytes!("../../../userspace/bin/spinner.bin");

/// Filetest — creates a file, writes to it, reads it back.
pub static FILETEST: &[u8] =
    include_bytes!("../../../userspace/bin/filetest.bin");

/// Hello Rust — "Hello from Rust on OxideOS!" (compiled from Rust/no_std).
pub static HELLO_RUST: &[u8] =
    include_bytes!("../../../userspace/bin/hello_rust.elf");

/// sh — minimal userspace shell (fork/exec/waitpid, built-in ls/cat/echo).
pub static SH: &[u8] =
    include_bytes!("../../../userspace/bin/sh.elf");

/// terminal — userspace GUI terminal (compositor IPC, pipe support).
pub static TERMINAL: &[u8] =
    include_bytes!("../../../userspace/bin/terminal.elf");

// ── Coreutils ────────────────────────────────────────────────────────────────

/// ls — list directory contents.
pub static LS: &[u8] =
    include_bytes!("../../../userspace/bin/ls.elf");

/// cat — concatenate and print files.
pub static CAT: &[u8] =
    include_bytes!("../../../userspace/bin/cat.elf");

/// ps — show running processes.
pub static PS: &[u8] =
    include_bytes!("../../../userspace/bin/ps.elf");

/// cp — copy a file.
pub static CP: &[u8] =
    include_bytes!("../../../userspace/bin/cp.elf");

/// mkdir — create a directory.
pub static MKDIR: &[u8] =
    include_bytes!("../../../userspace/bin/mkdir.elf");

/// pwd — print working directory.
pub static PWD: &[u8] =
    include_bytes!("../../../userspace/bin/pwd.elf");

/// wget — minimal HTTP/1.0 GET client (TCP socket, interactive).
pub static WGET: &[u8] =
    include_bytes!("../../../userspace/bin/wget.elf");

/// edit — nano-like full-screen text editor (compositor IPC, Ctrl+S/Q).
pub static EDIT: &[u8] =
    include_bytes!("../../../userspace/bin/edit.elf");

/// nc — minimal netcat: TCP/UDP listen and connect.
pub static NC: &[u8] =
    include_bytes!("../../../userspace/bin/nc.elf");

/// rm — remove a file.
pub static RM: &[u8] =
    include_bytes!("../../../userspace/bin/rm.elf");

/// mv — rename/move a file.
pub static MV: &[u8] =
    include_bytes!("../../../userspace/bin/mv.elf");

/// filemanager — GUI file manager with directory navigation.
pub static FILEMANAGER: &[u8] =
    include_bytes!("../../../userspace/bin/filemanager.elf");

// ── New coreutils (Phase 10.5) ───────────────────────────────────────────────

/// echo — print arguments.
pub static ECHO: &[u8] =
    include_bytes!("../../../userspace/bin/echo.elf");

/// grep — filter lines matching a pattern.
pub static GREP: &[u8] =
    include_bytes!("../../../userspace/bin/grep.elf");

/// wc — word, line, and byte count.
pub static WC: &[u8] =
    include_bytes!("../../../userspace/bin/wc.elf");

/// head — output first N lines.
pub static HEAD: &[u8] =
    include_bytes!("../../../userspace/bin/head.elf");

/// tail — output last N lines.
pub static TAIL: &[u8] =
    include_bytes!("../../../userspace/bin/tail.elf");

/// sort — sort lines.
pub static SORT: &[u8] =
    include_bytes!("../../../userspace/bin/sort.elf");

/// sleep — pause for N seconds.
pub static SLEEP: &[u8] =
    include_bytes!("../../../userspace/bin/sleep.elf");

/// kill — send signal to process.
pub static KILL: &[u8] =
    include_bytes!("../../../userspace/bin/kill.elf");

/// touch — create file if not exists.
pub static TOUCH: &[u8] =
    include_bytes!("../../../userspace/bin/touch.elf");

/// true — exit 0.
pub static TRUE: &[u8] =
    include_bytes!("../../../userspace/bin/true.elf");

/// false — exit 1.
pub static FALSE: &[u8] =
    include_bytes!("../../../userspace/bin/false.elf");

/// hello_c — "Hello from C on OxideOS!" compiled from C with gcc (Linux syscall ABI).
pub static HELLO_C: &[u8] =
    include_bytes!("../../../userspace/bin/hello_c.elf");

/// install — interactive disk installer; writes OxideOS to the secondary ATA disk.
pub static INSTALL: &[u8] =
    include_bytes!("../../../userspace/bin/install.elf");

/// hello_musl — "Hello from musl libc!" compiled with musl-gcc -static.
pub static HELLO_MUSL: &[u8] =
    include_bytes!("../../../userspace/bin/hello_musl.elf");

/// musl_test — tests malloc/free, envp, clock_gettime, getcwd via musl libc.
pub static MUSL_TEST: &[u8] =
    include_bytes!("../../../userspace/bin/musl_test.elf");

/// lua — Lua 5.4.7 interpreter compiled with musl-gcc -static.
pub static LUA: &[u8] =
    include_bytes!("../../../userspace/bin/lua.elf");

/// busybox — BusyBox 1.36.1 compiled with musl-gcc -static.
pub static BUSYBOX: &[u8] =
    include_bytes!("../../../userspace/bin/busybox.elf");

/// bash — GNU Bash 5.2 compiled with musl-gcc -static.
pub static BASH: &[u8] =
    include_bytes!("../../../userspace/bin/bash.elf");

/// Look up a built-in program by name.
pub fn find(name: &str) -> Option<&'static [u8]> {
    match name {
        "hello"     => Some(HELLO),
        "counter"   => Some(COUNTER),
        "sysinfo"   => Some(SYSINFO),
        "input"     => Some(INPUT),
        "fib"       => Some(FIB),
        "primes"    => Some(PRIMES),
        "countdown" => Some(COUNTDOWN),
        "spinner"   => Some(SPINNER),
        "filetest"  => Some(FILETEST),
        "hello_rust" => Some(HELLO_RUST),
        "sh"        => Some(SH),
        "terminal"  => Some(TERMINAL),
        "ls"        => Some(LS),
        "cat"       => Some(CAT),
        "ps"        => Some(PS),
        "cp"        => Some(CP),
        "mkdir"     => Some(MKDIR),
        "pwd"       => Some(PWD),
        "wget"      => Some(WGET),
        "edit"      => Some(EDIT),
        "nc"        => Some(NC),
        "rm"          => Some(RM),
        "mv"          => Some(MV),
        "filemanager" => Some(FILEMANAGER),
        "echo"  => Some(ECHO),
        "grep"  => Some(GREP),
        "wc"    => Some(WC),
        "head"  => Some(HEAD),
        "tail"  => Some(TAIL),
        "sort"  => Some(SORT),
        "sleep" => Some(SLEEP),
        "kill"  => Some(KILL),
        "touch" => Some(TOUCH),
        "true"    => Some(TRUE),
        "false"   => Some(FALSE),
        "hello_c" => Some(HELLO_C),
        "install"    => Some(INSTALL),
        "hello_musl" => Some(HELLO_MUSL),
        "musl_test"  => Some(MUSL_TEST),
        "lua"        => Some(LUA),
        "busybox"    => Some(BUSYBOX),
        "bash"       => Some(BASH),
        _            => None,
    }
}

/// List of available program names (shown by `run` with no arguments).
pub const NAMES: &[&str] = &[
    "hello", "counter", "sysinfo", "input",
    "fib", "primes", "countdown", "spinner", "filetest",
    "hello_rust", "sh", "terminal",
    "ls", "cat", "ps", "cp", "mkdir", "pwd", "wget", "edit", "nc", "rm", "mv",
    "filemanager",
    "echo", "grep", "wc", "head", "tail", "sort", "sleep", "kill", "touch",
    "true", "false",
    "hello_c",
    "install",
    "hello_musl",
    "musl_test",
    "lua",
    "busybox",
    "bash",
];
