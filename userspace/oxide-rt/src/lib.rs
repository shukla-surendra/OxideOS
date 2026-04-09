//! oxide-rt — Minimal runtime library for OxideOS user-space programs.
//!
//! Provides:
//!  - Raw `syscall!` macro for `int 0x80` ABI
//!  - High-level syscall wrappers (print, exit, sleep, …)
//!  - `#[panic_handler]` that prints the panic message and exits with code 1
//!  - `_start` entry point that calls the program's `fn oxide_main()`
//!
//! # Usage
//! In your program crate:
//! ```rust
//! #![no_std]
//! #![no_main]
//! use oxide_rt::println;
//!
//! #[no_mangle]
//! pub extern "C" fn oxide_main() {
//!     println!("Hello from Rust!");
//! }
//! ```

#![no_std]

use core::fmt::{self, Write};

// ── Raw syscall ABI ──────────────────────────────────────────────────────────
// rax = syscall number
// rdi, rsi, rdx, r10, r8, r9 = arguments
// return value in rax

#[macro_export]
macro_rules! syscall {
    ($nr:expr) => { $crate::raw::syscall0($nr) };
    ($nr:expr, $a1:expr) => { $crate::raw::syscall1($nr, $a1 as u64) };
    ($nr:expr, $a1:expr, $a2:expr) => { $crate::raw::syscall2($nr, $a1 as u64, $a2 as u64) };
    ($nr:expr, $a1:expr, $a2:expr, $a3:expr) => {
        $crate::raw::syscall3($nr, $a1 as u64, $a2 as u64, $a3 as u64)
    };
}

pub mod raw {
    #[inline(always)]
    pub unsafe fn syscall0(nr: u64) -> i64 {
        let ret: i64;
        unsafe {
            core::arch::asm!(
                "int 0x80",
                inlateout("rax") nr => ret,
                options(nostack)
            );
        }
        ret
    }

    #[inline(always)]
    pub unsafe fn syscall1(nr: u64, a1: u64) -> i64 {
        let ret: i64;
        unsafe {
            core::arch::asm!(
                "int 0x80",
                inlateout("rax") nr => ret,
                in("rdi") a1,
                options(nostack)
            );
        }
        ret
    }

    #[inline(always)]
    pub unsafe fn syscall2(nr: u64, a1: u64, a2: u64) -> i64 {
        let ret: i64;
        unsafe {
            core::arch::asm!(
                "int 0x80",
                inlateout("rax") nr => ret,
                in("rdi") a1,
                in("rsi") a2,
                options(nostack)
            );
        }
        ret
    }

    #[inline(always)]
    pub unsafe fn syscall3(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
        let ret: i64;
        unsafe {
            core::arch::asm!(
                "int 0x80",
                inlateout("rax") nr => ret,
                in("rdi") a1,
                in("rsi") a2,
                in("rdx") a3,
                options(nostack)
            );
        }
        ret
    }
}

// ── Syscall numbers ──────────────────────────────────────────────────────────
pub mod sys {
    pub const EXIT:    u64 = 0;
    pub const READ:    u64 = 20;
    pub const WRITE:   u64 = 21;
    pub const OPEN:    u64 = 22;
    pub const CLOSE:   u64 = 23;
    pub const PRINT:   u64 = 30;
    pub const GETCHAR: u64 = 31;
    pub const GETTIME: u64 = 40;
    pub const SLEEP:   u64 = 41;
}

// ── High-level wrappers ──────────────────────────────────────────────────────

/// Print raw bytes to the console.
#[inline]
pub fn print_bytes(s: &[u8]) {
    unsafe { raw::syscall2(sys::PRINT, s.as_ptr() as u64, s.len() as u64) };
}

/// Print a `&str` to the console.
#[inline]
pub fn print_str(s: &str) {
    print_bytes(s.as_bytes());
}

/// Exit the current process.
#[inline]
pub fn exit(code: i32) -> ! {
    unsafe { raw::syscall1(sys::EXIT, code as u64) };
    loop {} // unreachable, satisfies `-> !`
}

/// Sleep for `ms` milliseconds.
#[inline]
pub fn sleep_ms(ms: u64) {
    unsafe { raw::syscall1(sys::SLEEP, ms) };
}

/// Read one character from stdin. Returns `None` if the buffer is empty.
#[inline]
pub fn getchar() -> Option<u8> {
    let r = unsafe { raw::syscall0(sys::GETCHAR) };
    if r < 0 { None } else { Some(r as u8) }
}

/// Return the current timer tick count.
#[inline]
pub fn get_time() -> u64 {
    unsafe { raw::syscall0(sys::GETTIME) as u64 }
}

/// Open a file. `flags`: 1 = read, 2 = write/create.
/// Returns a file descriptor ≥ 3 on success, negative on error.
#[inline]
pub fn open(path: &str, flags: u32) -> i32 {
    let r = unsafe {
        raw::syscall3(sys::OPEN, path.as_ptr() as u64, path.len() as u64, flags as u64)
    };
    r as i32
}

/// Close a file descriptor.
#[inline]
pub fn close(fd: i32) {
    unsafe { raw::syscall1(sys::CLOSE, fd as u64) };
}

/// Write bytes to an open fd (≥ 3). Returns bytes written.
#[inline]
pub fn write(fd: i32, buf: &[u8]) -> i64 {
    unsafe { raw::syscall3(sys::WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) }
}

/// Read bytes from an open fd. Returns bytes read.
#[inline]
pub fn read(fd: i32, buf: &mut [u8]) -> i64 {
    unsafe { raw::syscall3(sys::READ, fd as u64, buf.as_ptr() as u64, buf.len() as u64) }
}

// ── Formatted printing ───────────────────────────────────────────────────────

/// A zero-allocation `Write` sink that flushes to the console byte-by-byte
/// using a small stack buffer.
pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        print_str(s);
        Ok(())
    }
}

/// Print formatted output to the console (no newline).
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::Console, $($arg)*);
    }};
}

/// Print formatted output to the console with a trailing newline.
#[macro_export]
macro_rules! println {
    ()            => { $crate::print_str("\n"); };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::Console, $($arg)*);
        $crate::print_str("\n");
    }};
}

// ── Entry point & panic handler ──────────────────────────────────────────────

unsafe extern "C" {
    /// The program must define this function.
    fn oxide_main();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    unsafe { oxide_main(); }
    exit(0);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    let _ = write!(Console, "\nPANIC: {}\n", info);
    exit(1);
}
