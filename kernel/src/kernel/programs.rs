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
        "filetest"   => Some(FILETEST),
        "hello_rust" => Some(HELLO_RUST),
        "sh"         => Some(SH),
        _            => None,
    }
}

/// List of available program names (shown by `run` with no arguments).
pub const NAMES: &[&str] = &[
    "hello", "counter", "sysinfo", "input",
    "fib", "primes", "countdown", "spinner", "filetest",
    "hello_rust", "sh",
];
