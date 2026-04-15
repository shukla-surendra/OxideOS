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

extern crate alloc;

use core::fmt::{self, Write};

// ── Global bump allocator ─────────────────────────────────────────────────────
//
// Provides a `#[global_allocator]` for all OxideOS user-space programs so they
// can freely use `alloc::vec::Vec`, `alloc::string::String`, `Box`, etc.
//
// Strategy: simple bump allocator backed by the `brk` syscall.
// Memory is never individually freed; the kernel reclaims everything on exit.
// Alignment is always satisfied by rounding the bump pointer up.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

struct BumpAlloc;

/// Virtual address of the next free byte in the heap.
/// Value 0 means "not yet initialised — call brk(0) on first use".
static HEAP_PTR: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size  = layout.size();
        if size == 0 { return align as *mut u8; }   // ZST: return non-null

        // Lazy-initialise: ask the kernel for the current break.
        let current = {
            let c = HEAP_PTR.load(Ordering::Relaxed);
            if c == 0 {
                let base = unsafe { raw::syscall1(sys::BRK, 0) };
                if base <= 0 { return core::ptr::null_mut(); }
                HEAP_PTR.store(base as usize, Ordering::Relaxed);
                base as usize
            } else {
                c
            }
        };

        // Align the bump pointer up.
        let aligned  = (current + align - 1) & !(align - 1);
        let new_end  = aligned + size;

        // Extend the kernel's heap break to cover the new allocation.
        let result = unsafe { raw::syscall1(sys::BRK, new_end as u64) };
        if result < new_end as i64 {
            return core::ptr::null_mut();           // OOM
        }

        HEAP_PTR.store(new_end, Ordering::Relaxed);
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator — individual frees are no-ops.
        // All memory is reclaimed when the process exits.
    }
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc;

// Re-export the most common alloc types so programs can write
// `use oxide_rt::alloc_prelude::*` instead of `extern crate alloc`.
pub mod alloc_prelude {
    pub use alloc::boxed::Box;
    pub use alloc::string::{String, ToString};
    pub use alloc::vec::Vec;
    pub use alloc::format;
}

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

    #[inline(always)]
    pub unsafe fn syscall5(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
        let ret: i64;
        unsafe {
            core::arch::asm!(
                "int 0x80",
                inlateout("rax") nr => ret,
                in("rdi") a1,
                in("rsi") a2,
                in("rdx") a3,
                in("r10") a4,
                in("r8")  a5,
                options(nostack)
            );
        }
        ret
    }
}

// ── Syscall numbers ──────────────────────────────────────────────────────────
pub mod sys {
    pub const EXIT:    u64 = 0;
    pub const MMAP:    u64 = 9;
    pub const MUNMAP:  u64 = 10;
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
    pub const STAT:     u64 = 74;
    pub const FSTAT:    u64 = 75;
    pub const UNLINK:   u64 = 76;
    pub const RENAME:   u64 = 77;
    pub const TRUNCATE: u64 = 78;
    // Socket syscalls
    pub const SOCKET:       u64 = 100;
    pub const BIND:         u64 = 101;
    pub const CONNECT:      u64 = 102;
    pub const LISTEN:       u64 = 103;
    pub const ACCEPT:       u64 = 104;
    pub const SEND:         u64 = 105;
    pub const RECV:         u64 = 106;
    pub const CLOSE_SOCKET: u64 = 107;
    pub const SENDTO:       u64 = 108;
    pub const RECVFROM:     u64 = 109;
    pub const DUP2:    u64 = 81;
    pub const KILL:    u64 = 91;
    pub const MSGQ_CREATE:  u64 = 115;
    pub const MSGSND:       u64 = 116;
    pub const MSGRCV:       u64 = 117;
    pub const MSGQ_DESTROY: u64 = 118;
    pub const MSGRCV_WAIT:  u64 = 119;
    pub const MSGQ_LEN:     u64 = 120;
    pub const IOCTL:        u64 = 92;
    pub const SIGACTION:    u64 = 93;
    pub const SIGRETURN:    u64 = 95;
    pub const SHMGET:       u64 = 110;
    pub const SHMAT:        u64 = 111;
    pub const SHMDT:        u64 = 112;
    pub const CHMOD:        u64 = 96;
    pub const CHOWN:        u64 = 97;
}

// ── TTY / termios structs ─────────────────────────────────────────────────────

/// POSIX termios (matching the kernel layout).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_line:  u8,
    pub c_cc: [u8; 32],
    _pad: [u8; 3],
}

impl Termios {
    pub const fn zeroed() -> Self {
        Self { c_iflag: 0, c_oflag: 0, c_cflag: 0, c_lflag: 0,
               c_line: 0, c_cc: [0u8; 32], _pad: [0u8; 3] }
    }
}

/// Terminal window size returned by TIOCGWINSZ.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Winsize {
    pub ws_row:    u16,
    pub ws_col:    u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

/// ioctl request codes.
pub mod ioctl {
    pub const TCGETS:     u64 = 0x5401;
    pub const TCSETS:     u64 = 0x5402;
    pub const TCSETSW:    u64 = 0x5403;
    pub const TCSETSF:    u64 = 0x5404;
    pub const TIOCGPGRP:  u64 = 0x540F;
    pub const TIOCSPGRP:  u64 = 0x5410;
    pub const TIOCGWINSZ: u64 = 0x5413;
    pub const TIOCSWINSZ: u64 = 0x5414;
}

/// Perform an ioctl on `fd`. `arg` is typically a pointer to a struct.
#[inline]
pub fn ioctl(fd: i32, request: u64, arg: u64) -> i64 {
    unsafe { raw::syscall3(sys::IOCTL, fd as u64, request, arg) }
}

// ── Signal handling ───────────────────────────────────────────────────────────

/// Standard signal numbers.
pub mod sig {
    pub const SIGHUP:  u32 = 1;
    pub const SIGINT:  u32 = 2;
    pub const SIGQUIT: u32 = 3;
    pub const SIGKILL: u32 = 9;
    pub const SIGTERM: u32 = 15;
    pub const SIGCHLD: u32 = 17;
    pub const SIGCONT: u32 = 18;
    pub const SIGSTOP: u32 = 19;
}

/// Default signal action (terminate the process for most signals).
pub const SIG_DFL: u64 = 0;
/// Ignore this signal.
pub const SIG_IGN: u64 = 1;

/// Type alias for a signal handler function pointer.
pub type SigHandler = unsafe extern "C" fn(signum: i32);

/// Register a signal handler for `signum`.
/// `handler` is the user-space function to call, or `SIG_DFL`/`SIG_IGN`.
/// Returns 0 on success, negative on error.
#[inline]
pub fn sigaction(signum: u32, handler: u64) -> i64 {
    unsafe { raw::syscall3(sys::SIGACTION, signum as u64, handler, 0) }
}

/// Restore the context saved before a signal handler was called.
/// Programs should not normally call this directly — the trampoline handles it.
#[inline]
pub fn sigreturn() -> i64 {
    unsafe { raw::syscall1(sys::SIGRETURN, 0) }
}

/// Send signal `signum` to process `pid`. Returns 0 on success.
#[inline]
pub fn kill_signal(pid: u32, signum: u32) -> i64 {
    unsafe { raw::syscall2(sys::KILL, pid as u64, signum as u64) }
}

// ── Shared memory ─────────────────────────────────────────────────────────────

/// Create or open a shared memory segment identified by `key`.
/// `size` is the minimum size in bytes.
/// Returns the segment id (≥ 0) or a negative error code.
#[inline]
pub fn shmget(key: u32, size: usize) -> i64 {
    unsafe { raw::syscall3(sys::SHMGET, key as u64, size as u64, 0x0200) } // IPC_CREAT
}

/// Attach shared memory segment `shmid` into this process.
/// Returns the virtual address as a positive i64, or negative on error.
#[inline]
pub fn shmat(shmid: u32) -> i64 {
    unsafe { raw::syscall2(sys::SHMAT, shmid as u64, 0) }
}

/// Detach the shared memory segment previously mapped at `addr`.
#[inline]
pub fn shmdt(addr: usize) -> i64 {
    unsafe { raw::syscall1(sys::SHMDT, addr as u64) }
}

// ── File permissions ──────────────────────────────────────────────────────────

/// Change permission bits on `path` (RamFS only for now).
/// `mode` is a POSIX octal mode, e.g. `0o644`.
#[inline]
pub fn chmod(path: &str, mode: u16) -> i64 {
    let b = path.as_bytes();
    unsafe { raw::syscall3(sys::CHMOD, b.as_ptr() as u64, b.len() as u64, mode as u64) }
}

/// Change owner and group of `path`.
#[inline]
pub fn chown(path: &str, uid: u32, gid: u32) -> i64 {
    let b = path.as_bytes();
    unsafe { raw::syscall4(sys::CHOWN, b.as_ptr() as u64, b.len() as u64, uid as u64, gid as u64) }
}

/// Remove (unlink) a file at `path`.  Returns 0 on success.
#[inline]
pub fn unlink(path: &str) -> i64 {
    let b = path.as_bytes();
    unsafe { raw::syscall2(sys::UNLINK, b.as_ptr() as u64, b.len() as u64) }
}

/// Rename/move a file.  Returns 0 on success.
#[inline]
pub fn rename(old_path: &str, new_path: &str) -> i64 {
    let ob = old_path.as_bytes();
    let nb = new_path.as_bytes();
    unsafe {
        raw::syscall4(sys::RENAME,
            ob.as_ptr() as u64, ob.len() as u64,
            nb.as_ptr() as u64, nb.len() as u64)
    }
}

/// Truncate an open file descriptor to `length` bytes.  Returns 0 on success.
#[inline]
pub fn truncate(fd: i32, length: u64) -> i64 {
    unsafe { raw::syscall2(sys::TRUNCATE, fd as u64, length) }
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

/// Map `len` bytes of anonymous zeroed memory (MAP_ANONYMOUS|MAP_PRIVATE).
/// Returns a pointer to the mapped region, or null on failure.
/// The region persists until the process exits (munmap is a no-op).
#[inline]
pub fn mmap_anon(len: usize) -> *mut u8 {
    let r = unsafe { raw::syscall2(sys::MMAP, 0u64, len as u64) };
    if r <= 0 { core::ptr::null_mut() } else { r as *mut u8 }
}

/// Unmap a previously mapped region. Currently a no-op stub.
#[inline]
pub fn munmap(_ptr: *mut u8, _len: usize) -> i64 {
    unsafe { raw::syscall2(sys::MUNMAP, _ptr as u64, _len as u64) }
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

/// Minimal file metadata returned by `stat` / `fstat`.
#[repr(C)]
pub struct FileStat {
    /// File size in bytes.
    pub size: u64,
    /// Entry type: 0 = file, 1 = directory, 2 = device.
    pub kind: u32,
    pub _pad: u32,
}

impl FileStat {
    pub const KIND_FILE: u32 = 0;
    pub const KIND_DIR:  u32 = 1;
    pub const KIND_DEV:  u32 = 2;

    pub const fn zeroed() -> Self {
        Self { size: 0, kind: 0, _pad: 0 }
    }

    pub fn is_file(&self) -> bool { self.kind == Self::KIND_FILE }
    pub fn is_dir(&self)  -> bool { self.kind == Self::KIND_DIR  }
}

/// Stat `path`.  Fills `*out` and returns 0 on success.
#[inline]
pub fn stat(path: &str, out: &mut FileStat) -> i64 {
    unsafe {
        raw::syscall3(sys::STAT,
            path.as_ptr() as u64,
            path.len() as u64,
            out as *mut FileStat as u64)
    }
}

/// Stat an open file descriptor.  Fills `*out` and returns 0 on success.
#[inline]
pub fn fstat(fd: i32, out: &mut FileStat) -> i64 {
    unsafe {
        raw::syscall2(sys::FSTAT, fd as u64, out as *mut FileStat as u64)
    }
}

// ── Networking ───────────────────────────────────────────────────────────────

/// AF_INET socket address (mirrors `struct sockaddr_in`, 16 bytes).
#[repr(C)]
pub struct SockAddrIn {
    pub sin_family: u16,  // 2 = AF_INET
    pub sin_port:   u16,  // big-endian port
    pub sin_addr:   [u8; 4], // IPv4 address (big-endian)
    pub _pad:       [u8; 8],
}

impl SockAddrIn {
    pub fn new(ip: [u8; 4], port: u16) -> Self {
        Self {
            sin_family: 2u16.to_le(),
            sin_port:   port.to_be(),
            sin_addr:   ip,
            _pad:       [0; 8],
        }
    }
}

pub const AF_INET:     u32 = 2;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM:  u32 = 2;

/// Create a socket. Returns socket fd (≥ 200) on success.
#[inline]
pub fn socket(domain: u32, sock_type: u32, protocol: u32) -> i64 {
    unsafe { raw::syscall3(sys::SOCKET, domain as u64, sock_type as u64, protocol as u64) }
}

/// Connect to a remote address. Returns 0 on success.
#[inline]
pub fn connect(sfd: i64, addr: &SockAddrIn) -> i64 {
    unsafe {
        raw::syscall3(sys::CONNECT,
            sfd as u64,
            addr as *const SockAddrIn as u64,
            core::mem::size_of::<SockAddrIn>() as u64)
    }
}

/// Send data on a connected socket. Returns bytes sent.
#[inline]
pub fn send(sfd: i64, buf: &[u8]) -> i64 {
    unsafe { raw::syscall3(sys::SEND, sfd as u64, buf.as_ptr() as u64, buf.len() as u64) }
}

/// Receive data from a socket. Returns bytes received, 0 on EOF, -11 if no data yet.
#[inline]
pub fn recv(sfd: i64, buf: &mut [u8]) -> i64 {
    unsafe { raw::syscall3(sys::RECV, sfd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

/// Close a socket.
#[inline]
pub fn close_socket(sfd: i64) -> i64 {
    unsafe { raw::syscall1(sys::CLOSE_SOCKET, sfd as u64) }
}

/// Bind a socket to a local address.
#[inline]
pub fn bind(sfd: i64, addr: &SockAddrIn) -> i64 {
    unsafe {
        raw::syscall3(sys::BIND,
            sfd as u64,
            addr as *const SockAddrIn as u64,
            core::mem::size_of::<SockAddrIn>() as u64)
    }
}

/// Put a TCP socket into passive listen mode.
#[inline]
pub fn listen(sfd: i64, backlog: i32) -> i64 {
    unsafe { raw::syscall2(sys::LISTEN, sfd as u64, backlog as u64) }
}

/// Accept a connection on a listening TCP socket.
/// Returns a new socket fd on success, -11 (EAGAIN) if no connection is ready.
#[inline]
pub fn accept(sfd: i64) -> i64 {
    unsafe { raw::syscall1(sys::ACCEPT, sfd as u64) }
}

/// Send a datagram to a specific address (UDP).
/// Kernel dispatch: arg1=sfd, arg2=buf, arg3=len, arg4=flags, arg5=addr_ptr (addr_len=16 implicit).
#[inline]
pub fn sendto(sfd: i64, buf: &[u8], addr: &SockAddrIn) -> i64 {
    unsafe {
        raw::syscall5(sys::SENDTO,
            sfd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64,
            0u64, // flags
            addr as *const SockAddrIn as u64)
    }
}

/// Receive a datagram and optionally capture source address (UDP).
/// Kernel dispatch: arg1=sfd, arg2=buf, arg3=len, arg4=flags, arg5=addr_ptr.
/// If `src` is `Some`, the kernel fills it with the sender's address.
#[inline]
pub fn recvfrom(sfd: i64, buf: &mut [u8], src: Option<&mut SockAddrIn>) -> i64 {
    let addr_ptr = src.map(|a| a as *mut SockAddrIn as u64).unwrap_or(0);
    unsafe {
        raw::syscall5(sys::RECVFROM,
            sfd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            0u64, // flags
            addr_ptr)
    }
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

/// Blit an ARGB framebuffer stored in shared memory segment `shmid` to the window.
///
/// `src_x/y` — top-left in the shm buffer; `src_w/h` — size of the region to blit.
/// `dst_x/y` — top-left in the window content area.
/// `stride`  — bytes per row in the shm buffer (typically `src_width * 4`).
pub fn comp_blit_shm(
    shmid: u32,
    src_x: u32, src_y: u32, src_w: u32, src_h: u32,
    dst_x: u32, dst_y: u32,
    stride: u32,
) {
    let mut d = [0u8; 32];
    u32_le(shmid,  &mut d, 0);
    u32_le(src_x,  &mut d, 4);
    u32_le(src_y,  &mut d, 8);
    u32_le(src_w,  &mut d, 12);
    u32_le(src_h,  &mut d, 16);
    u32_le(dst_x,  &mut d, 20);
    u32_le(dst_y,  &mut d, 24);
    u32_le(stride, &mut d, 28);
    msgsnd(COMPOSITOR_QID, 5, &d); // MSG_BLIT_SHM = 5
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
