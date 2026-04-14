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
    pub unsafe fn syscall4(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
        let ret: i64;
        unsafe {
            core::arch::asm!(
                "int 0x80",
                inlateout("rax") nr => ret,
                in("rdi") a1,
                in("rsi") a2,
                in("rdx") a3,
                in("r10") a4,
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
    pub const FORK:    u64 = 1;
    pub const WAIT:    u64 = 2;
    pub const GETPID:  u64 = 3;
    pub const EXEC:    u64 = 5;
    pub const BRK:     u64 = 11;
    pub const READ:    u64 = 20;
    pub const WRITE:   u64 = 21;
    pub const OPEN:    u64 = 22;
    pub const CLOSE:   u64 = 23;
    pub const PRINT:   u64 = 30;
    pub const GETCHAR: u64 = 31;
    pub const GETTIME: u64 = 40;
    pub const SLEEP:   u64 = 41;
    pub const PIPE:    u64 = 60;
    pub const READDIR: u64 = 70;
    pub const MKDIR:   u64 = 71;
    pub const CHDIR:   u64 = 72;
    pub const GETCWD:  u64 = 73;
    pub const DUP2:    u64 = 81;
    pub const KILL:    u64 = 91;
    pub const MSGQ_CREATE:  u64 = 115;
    pub const MSGSND:       u64 = 116;
    pub const MSGRCV:       u64 = 117;
    pub const MSGQ_DESTROY: u64 = 118;
    pub const MSGRCV_WAIT:  u64 = 119;
    pub const MSGQ_LEN:     u64 = 120;
}

// ── High-level wrappers ──────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub struct IpcMessage {
    pub type_id: u32,
    pub size: u32,
    pub data: [u8; 256],
}

impl IpcMessage {
    pub const fn empty() -> Self {
        Self { type_id: 0, size: 0, data: [0; 256] }
    }
}

/// Create or open a message queue. Returns the queue ID on success, or a negative error code.
#[inline]
pub fn msgq_create(id: u32) -> i64 {
    unsafe { raw::syscall1(sys::MSGQ_CREATE, id as u64) }
}

/// Send a message to the specified queue. Returns 0 on success, or a negative error code.
#[inline]
pub fn msgsnd(id: u32, type_id: u32, data: &[u8]) -> i64 {
    unsafe { raw::syscall4(sys::MSGSND, id as u64, type_id as u64, data.as_ptr() as u64, data.len() as u64) }
}

/// Non-blocking receive. Returns 0 on success, -EAGAIN if the queue is empty.
#[inline]
pub fn msgrcv(id: u32, msg_out: &mut IpcMessage) -> i64 {
    unsafe { raw::syscall2(sys::MSGRCV, id as u64, msg_out as *mut IpcMessage as u64) }
}

/// Destroy a message queue. Returns 0 on success or a negative error code.
#[inline]
pub fn msgq_destroy(id: u32) -> i64 {
    unsafe { raw::syscall1(sys::MSGQ_DESTROY, id as u64) }
}

/// Blocking receive. The task sleeps until a message is available.
/// Returns 0 on success or a negative error code.
#[inline]
pub fn msgrcv_wait(id: u32, msg_out: &mut IpcMessage) -> i64 {
    unsafe { raw::syscall2(sys::MSGRCV_WAIT, id as u64, msg_out as *mut IpcMessage as u64) }
}

/// Returns the number of pending messages in the queue, or a negative error code.
#[inline]
pub fn msgq_len(id: u32) -> i64 {
    unsafe { raw::syscall1(sys::MSGQ_LEN, id as u64) }
}

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

/// Return the current process PID.
#[inline]
pub fn getpid() -> u32 {
    unsafe { raw::syscall0(sys::GETPID) as u32 }
}

/// Fork the current process.  Returns child PID to parent, 0 to child.
/// Returns negative on error.
#[inline]
pub fn fork() -> i64 {
    unsafe { raw::syscall0(sys::FORK) }
}

/// Wait for child `pid` to exit.  Returns child's exit code.
#[inline]
pub fn waitpid(pid: u32) -> i64 {
    unsafe { raw::syscall1(sys::WAIT, pid as u64) }
}

/// Set the heap break to `new_end`.  Pass 0 to query current break.
/// Returns new (or current) break on success, negative on error.
#[inline]
pub fn brk(new_end: u64) -> i64 {
    unsafe { raw::syscall1(sys::BRK, new_end) }
}

/// Send SIGKILL to `pid`.  Returns 0 on success.
#[inline]
pub fn kill(pid: u32) -> i64 {
    unsafe { raw::syscall1(sys::KILL, pid as u64) }
}

/// Duplicate `old_fd` to `new_fd`.  Returns `new_fd` on success.
#[inline]
pub fn dup2(old_fd: i32, new_fd: i32) -> i64 {
    unsafe { raw::syscall2(sys::DUP2, old_fd as u64, new_fd as u64) }
}

/// Allocate an anonymous pipe.  On success writes the read and write FDs into
/// `*r` and `*w` respectively and returns 0.  Returns negative on error.
#[inline]
pub fn pipe(r: *mut i32, w: *mut i32) -> i64 {
    unsafe { raw::syscall2(sys::PIPE, r as u64, w as u64) }
}

/// Read directory entries from `path` into `buf`.
/// Each entry is `<name>\n` for files, `<name>/\n` for directories.
/// Returns bytes written, or negative on error.
#[inline]
pub fn readdir(path: &str, buf: &mut [u8]) -> i64 {
    unsafe {
        raw::syscall4(sys::READDIR,
            path.as_ptr() as u64, path.len() as u64,
            buf.as_mut_ptr() as u64, buf.len() as u64)
    }
}

/// Execute the program at `path`, replacing the current process image.
/// On success this never returns.
#[inline]
pub fn exec(path: &str) -> i64 {
    unsafe { raw::syscall2(sys::EXEC, path.as_ptr() as u64, path.len() as u64) }
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

/// Create a directory at `path`. Returns 0 on success.
#[inline]
pub fn mkdir(path: &str) -> i64 {
    unsafe { raw::syscall2(sys::MKDIR, path.as_ptr() as u64, path.len() as u64) }
}

/// Change the current working directory. Returns 0 on success.
#[inline]
pub fn chdir(path: &str) -> i64 {
    unsafe { raw::syscall2(sys::CHDIR, path.as_ptr() as u64, path.len() as u64) }
}

/// Get the current working directory into `buf`. Returns bytes written.
#[inline]
pub fn getcwd(buf: &mut [u8]) -> i64 {
    unsafe { raw::syscall2(sys::GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) }
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

// ── Compositor client helpers ────────────────────────────────────────────────
//
// Userspace programs draw into their window by sending IPC messages to the
// kernel compositor (queue ID 1).  All coordinates are relative to the
// window's content area top-left (0, 0).

pub const COMPOSITOR_QID: u32 = 1;

const MSG_FILL_RECT:  u32 = 1;
const MSG_DRAW_TEXT:  u32 = 2;
const MSG_PRESENT:    u32 = 3;
const MSG_CLEAR_RECT: u32 = 4;

fn u32_le(v: u32, buf: &mut [u8], off: usize) {
    buf[off]   = v as u8;
    buf[off+1] = (v >> 8)  as u8;
    buf[off+2] = (v >> 16) as u8;
    buf[off+3] = (v >> 24) as u8;
}

/// Fill a rectangle with `color` (0xAARRGGBB).
pub fn comp_fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) {
    let mut d = [0u8; 20];
    u32_le(x, &mut d, 0); u32_le(y, &mut d, 4);
    u32_le(w, &mut d, 8); u32_le(h, &mut d, 12);
    u32_le(color, &mut d, 16);
    msgsnd(COMPOSITOR_QID, MSG_FILL_RECT, &d);
}

/// Clear a rectangle to the window background colour.
pub fn comp_clear_rect(x: u32, y: u32, w: u32, h: u32) {
    let mut d = [0u8; 16];
    u32_le(x, &mut d, 0); u32_le(y, &mut d, 4);
    u32_le(w, &mut d, 8); u32_le(h, &mut d, 12);
    msgsnd(COMPOSITOR_QID, MSG_CLEAR_RECT, &d);
}

/// Draw a UTF-8 string.  `text` must be ≤ 236 bytes (IPC payload limit).
pub fn comp_draw_text(x: u32, y: u32, color: u32, text: &str) {
    let bytes = text.as_bytes();
    let len = bytes.len().min(236);
    let mut d = [0u8; 256];
    u32_le(x, &mut d, 0); u32_le(y, &mut d, 4);
    u32_le(color, &mut d, 8);
    u32_le(len as u32, &mut d, 12);
    d[16..16 + len].copy_from_slice(&bytes[..len]);
    msgsnd(COMPOSITOR_QID, MSG_DRAW_TEXT, &d[..16 + len]);
}

/// Signal to the compositor that the current frame is complete.
pub fn comp_present() {
    msgsnd(COMPOSITOR_QID, MSG_PRESENT, &[]);
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
