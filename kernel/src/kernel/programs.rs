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

/// Look up a built-in program by name.
pub fn find(name: &str) -> Option<&'static [u8]> {
    match name {
        "hello"   => Some(HELLO),
        "counter" => Some(COUNTER),
        "sysinfo" => Some(SYSINFO),
        "input"   => Some(INPUT),
        _         => None,
    }
}

/// List of available program names (shown by `run` with no arguments).
pub const NAMES: &[&str] = &["hello", "counter", "sysinfo", "input"];
