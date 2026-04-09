// src/kernel/syscall_core.rs
//! Core syscall types and dispatch rules.
//!
//! Written against `core` only so the same logic can be reused from the
//! kernel and from host-side unit-tests.

use core::{mem::size_of, ptr, slice};

pub const TIMER_HZ: u64 = 100;
pub const USER_SPACE_START: u64 = 0x1000;
pub const KERNEL_SPACE_BASE: u64 = 0xFFFF_8000_0000_0000;

// ── Syscall numbers ────────────────────────────────────────────────────────
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
    Exit          = 0,
    Fork          = 1,
    Wait          = 2,
    GetPid        = 3,
    Exec          = 5,
    Mmap          = 9,
    Munmap        = 10,
    Brk           = 11,
    Read          = 20,
    Write         = 21,
    Open          = 22,
    Close         = 23,
    Print         = 30,
    GetChar       = 31,
    GetTime       = 40,
    Sleep         = 41,
    GetSystemInfo = 50,
    Pipe          = 60,
    ReadDir       = 70,
    Dup2          = 81,
    Kill          = 91,
    Invalid       = u64::MAX,
}

impl Syscall {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exit          => "exit",
            Self::Fork          => "fork",
            Self::Wait          => "wait",
            Self::GetPid        => "getpid",
            Self::Exec          => "exec",
            Self::Mmap          => "mmap",
            Self::Munmap        => "munmap",
            Self::Brk           => "brk",
            Self::Read          => "read",
            Self::Write         => "write",
            Self::Open          => "open",
            Self::Close         => "close",
            Self::Print         => "print",
            Self::GetChar       => "getchar",
            Self::GetTime       => "gettime",
            Self::Sleep         => "sleep",
            Self::GetSystemInfo => "get_system_info",
            Self::Pipe          => "pipe",
            Self::ReadDir       => "readdir",
            Self::Dup2          => "dup2",
            Self::Kill          => "kill",
            Self::Invalid       => "invalid",
        }
    }
}

impl From<u64> for Syscall {
    fn from(num: u64) -> Self {
        match num {
            0  => Self::Exit,
            1  => Self::Fork,
            2  => Self::Wait,
            3  => Self::GetPid,
            5  => Self::Exec,
            9  => Self::Mmap,
            10 => Self::Munmap,
            11 => Self::Brk,
            20 => Self::Read,
            21 => Self::Write,
            22 => Self::Open,
            23 => Self::Close,
            30 => Self::Print,
            31 => Self::GetChar,
            40 => Self::GetTime,
            41 => Self::Sleep,
            50 => Self::GetSystemInfo,
            60 => Self::Pipe,
            70 => Self::ReadDir,
            81 => Self::Dup2,
            91 => Self::Kill,
            _  => Self::Invalid,
        }
    }
}

// ── Result / request types ─────────────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallResult {
    pub value: i64,
    pub error: bool,
}

impl SyscallResult {
    pub const fn ok(value: i64) -> Self { Self { value, error: false } }
    pub const fn err(error_code: i64) -> Self { Self { value: error_code, error: true } }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemInfo {
    pub total_memory:  u64,
    pub free_memory:   u64,
    pub uptime_ms:     u64,
    pub process_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallRequest {
    pub number: u64,
    pub arg1:   u64,
    pub arg2:   u64,
    pub arg3:   u64,
    pub arg4:   u64,
    pub arg5:   u64,
}

impl SyscallRequest {
    pub const fn new(number: u64, arg1: u64, arg2: u64,
                     arg3: u64, arg4: u64, arg5: u64) -> Self {
        Self { number, arg1, arg2, arg3, arg4, arg5 }
    }
    pub fn syscall(self) -> Syscall { Syscall::from(self.number) }
}

// ── Error codes ────────────────────────────────────────────────────────────
pub const EINVAL: i64 = -1;
pub const ENOSYS: i64 = -2;
pub const EACCES: i64 = -3;
pub const ENOMEM: i64 = -4;
pub const EBADF:  i64 = -5;
pub const EAGAIN: i64 = -6;
pub const ENOENT: i64 = -7;

// ── Runtime trait ──────────────────────────────────────────────────────────
pub trait SyscallRuntime {
    fn trace(&mut self, _syscall: Syscall) {}
    fn current_pid(&self) -> u64 { 1 }
    fn current_ticks(&self) -> u64;
    fn write_console(&mut self, bytes: &[u8]);

    fn fill_system_info(&self, info: &mut SystemInfo) {
        *info = SystemInfo {
            total_memory:  128 * 1024 * 1024,
            free_memory:   64  * 1024 * 1024,
            uptime_ms:     self.current_ticks() * 1000 / TIMER_HZ,
            process_count: 1,
        };
    }

    fn sleep_until_tick(&mut self, target_tick: u64);
    fn exit(&mut self, code: i32) -> !;

    // ── stdin ──────────────────────────────────────────────────────────────
    /// Pop one byte from the stdin ring. Returns EAGAIN (-6) if empty.
    fn get_char(&mut self) -> i64 { EAGAIN }

    // ── Process model ──────────────────────────────────────────────────────
    /// Replace the current process image with the ELF at `path`.
    /// On success this never returns (jumps directly into the new image).
    /// On failure it returns a negative error code.
    fn exec_program(&mut self, _path: &[u8]) -> i64 { ENOSYS }

    /// Fork the current process.  Returns child PID to parent, 0 to child.
    fn fork_child(&mut self) -> i64 { ENOSYS }

    /// Block until the child with `pid` exits; return its exit code.
    /// May never return if it blocks (calls exit_to_kernel internally).
    fn waitpid_impl(&mut self, _pid: u64) -> i64 { ENOSYS }

    /// Set/query the userspace heap break.  `new_end == 0` returns current end.
    fn brk_program(&mut self, _new_end: u64) -> i64 { ENOSYS }

    /// Terminate the process with the given PID.  Returns 0 on success.
    fn kill_pid(&mut self, _pid: u64) -> i64 { ENOSYS }

    /// Fill `buf` with newline-separated directory entries under `path`.
    /// Each entry is `<name>\n` for files, `<name>/\n` for directories.
    /// Returns bytes written, or negative on error.
    fn readdir_impl(&mut self, _path: &[u8], _buf: &mut [u8]) -> i64 { ENOSYS }

    /// Duplicate `old_fd` to `new_fd`.  Returns `new_fd` on success.
    fn dup2_impl(&mut self, _old_fd: i32, _new_fd: i32) -> i64 { ENOSYS }

    // ── Filesystem hooks (default: not supported) ──────────────────────────
    /// Open a file; `path` is raw bytes from user space.
    fn fs_open(&mut self, path: &[u8], flags: u32) -> i64 { ENOSYS }
    /// Close a file descriptor.
    fn fs_close(&mut self, fd: i32) -> i64 { ENOSYS }
    /// Read from a file descriptor into `buf`.
    fn fs_read(&mut self, fd: i32, buf: &mut [u8]) -> i64 { ENOSYS }
    /// Write `buf` to a file descriptor ≥ 3 (FD 1/2 go to `write_console`).
    fn fs_write_file(&mut self, fd: i32, buf: &[u8]) -> i64 { ENOSYS }

    // ── IPC ────────────────────────────────────────────────────────────────
    /// Allocate a pipe; write (read_fd, write_fd) to the two user pointers.
    fn pipe_alloc(&mut self, read_fd_ptr: u64, write_fd_ptr: u64) -> i64 { ENOSYS }
}

// ── Validation ─────────────────────────────────────────────────────────────
pub fn validate_user_range(ptr_addr: u64, len: u64) -> Result<(), i64> {
    if len == 0 { return Ok(()); }
    if ptr_addr < USER_SPACE_START || len > usize::MAX as u64 { return Err(EINVAL); }
    let end = ptr_addr.checked_add(len - 1).ok_or(EINVAL)?;
    if end >= KERNEL_SPACE_BASE { return Err(EINVAL); }
    Ok(())
}

unsafe fn write_user_value<T>(ptr_addr: u64, value: T) -> Result<(), i64> {
    validate_user_range(ptr_addr, size_of::<T>() as u64)?;
    unsafe { ptr::write_unaligned(ptr_addr as *mut T, value); }
    Ok(())
}

// ── Dispatcher ─────────────────────────────────────────────────────────────
pub unsafe fn dispatch<R: SyscallRuntime>(
    runtime: &mut R, request: SyscallRequest,
) -> SyscallResult {
    let syscall = request.syscall();
    runtime.trace(syscall);

    match syscall {
        Syscall::Exit          => runtime.exit(request.arg1 as i32),
        Syscall::Fork          => sys_fork(runtime),
        Syscall::Wait          => sys_waitpid(runtime, request.arg1),
        Syscall::Exec          => unsafe { sys_exec(runtime, request.arg1, request.arg2) },
        Syscall::GetPid        => SyscallResult::ok(runtime.current_pid() as i64),
        Syscall::Mmap          => SyscallResult::err(ENOSYS),
        Syscall::Munmap        => SyscallResult::err(ENOSYS),
        Syscall::Brk           => sys_brk(runtime, request.arg1),
        Syscall::Read          => unsafe { sys_read(runtime, request.arg1 as i32,
                                                    request.arg2, request.arg3) },
        Syscall::Write         => unsafe { sys_write(runtime, request.arg1 as i32,
                                                     request.arg2, request.arg3) },
        Syscall::Open          => unsafe { sys_open(runtime, request.arg1,
                                                    request.arg2, request.arg3) },
        Syscall::Close         => sys_close(runtime, request.arg1 as i32),
        Syscall::Print         => unsafe { sys_print(runtime, request.arg1, request.arg2) },
        Syscall::GetChar       => {
            let r = runtime.get_char();
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GetTime       => SyscallResult::ok(runtime.current_ticks() as i64),
        Syscall::Sleep         => sys_sleep(runtime, request.arg1),
        Syscall::GetSystemInfo => unsafe { sys_get_system_info(runtime, request.arg1) },
        Syscall::Pipe          => unsafe { sys_pipe(runtime, request.arg1, request.arg2) },
        Syscall::ReadDir       => unsafe { sys_readdir(runtime, request.arg1, request.arg2,
                                                        request.arg3, request.arg4) },
        Syscall::Dup2          => sys_dup2(runtime, request.arg1 as i32, request.arg2 as i32),
        Syscall::Kill          => sys_kill(runtime, request.arg1),
        Syscall::Invalid       => SyscallResult::err(ENOSYS),
    }
}

// ── Individual syscall implementations ────────────────────────────────────

unsafe fn sys_open<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64, flags: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(path_ptr, path_len) {
        return SyscallResult::err(code);
    }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let result = runtime.fs_open(path, flags as u32);
    if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) }
}

fn sys_close<R: SyscallRuntime>(runtime: &mut R, fd: i32) -> SyscallResult {
    let result = runtime.fs_close(fd);
    if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) }
}

unsafe fn sys_read<R: SyscallRuntime>(
    runtime: &mut R, fd: i32, buf_ptr: u64, count: u64,
) -> SyscallResult {
    // stdin: read one byte per call from the global ring buffer
    if fd == 0 {
        if count == 0 { return SyscallResult::ok(0); }
        if let Err(code) = validate_user_range(buf_ptr, 1) {
            return SyscallResult::err(code);
        }
        let r = runtime.get_char();
        if r < 0 { return SyscallResult::err(r); }
        unsafe { ptr::write(buf_ptr as *mut u8, r as u8); }
        return SyscallResult::ok(1);
    }
    if let Err(code) = validate_user_range(buf_ptr, count) {
        return SyscallResult::err(code);
    }
    let buf = unsafe { slice::from_raw_parts_mut(buf_ptr as *mut u8, count as usize) };
    let result = runtime.fs_read(fd, buf);
    if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) }
}

unsafe fn sys_write<R: SyscallRuntime>(
    runtime: &mut R, fd: i32, buf_ptr: u64, count: u64,
) -> SyscallResult {
    if fd == 1 || fd == 2 {
        // stdout / stderr → serial console
        if let Err(code) = validate_user_range(buf_ptr, count) {
            return SyscallResult::err(code);
        }
        let buf = unsafe { slice::from_raw_parts(buf_ptr as *const u8, count as usize) };
        runtime.write_console(buf);
        return SyscallResult::ok(count as i64);
    }
    if fd >= 3 {
        // File descriptor → RamFS
        if let Err(code) = validate_user_range(buf_ptr, count) {
            return SyscallResult::err(code);
        }
        let buf = unsafe { slice::from_raw_parts(buf_ptr as *const u8, count as usize) };
        let result = runtime.fs_write_file(fd, buf);
        return if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) };
    }
    SyscallResult::err(EBADF)
}

unsafe fn sys_print<R: SyscallRuntime>(
    runtime: &mut R, msg_ptr: u64, len: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(msg_ptr, len) {
        return SyscallResult::err(code);
    }
    let msg = unsafe { slice::from_raw_parts(msg_ptr as *const u8, len as usize) };
    runtime.write_console(msg);
    SyscallResult::ok(len as i64)
}

fn sys_sleep<R: SyscallRuntime>(runtime: &mut R, ms: u64) -> SyscallResult {
    let start = runtime.current_ticks();
    let ticks = ms.saturating_mul(TIMER_HZ) / 1000;
    runtime.sleep_until_tick(start.saturating_add(ticks));
    SyscallResult::ok(0)
}

unsafe fn sys_get_system_info<R: SyscallRuntime>(
    runtime: &mut R, info_ptr: u64,
) -> SyscallResult {
    let mut info = SystemInfo::default();
    runtime.fill_system_info(&mut info);
    match unsafe { write_user_value(info_ptr, info) } {
        Ok(())     => SyscallResult::ok(0),
        Err(code)  => SyscallResult::err(code),
    }
}

unsafe fn sys_exec<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(path_ptr, path_len) {
        return SyscallResult::err(code);
    }
    if path_len == 0 { return SyscallResult::err(EINVAL); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    // On success exec_program diverges (never returns).
    // On failure it returns a negative error code.
    let err = runtime.exec_program(path);
    SyscallResult::err(err)
}

unsafe fn sys_pipe<R: SyscallRuntime>(
    runtime: &mut R, read_fd_ptr: u64, write_fd_ptr: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(read_fd_ptr, 4) { return SyscallResult::err(code); }
    if let Err(code) = validate_user_range(write_fd_ptr, 4) { return SyscallResult::err(code); }
    let r = runtime.pipe_alloc(read_fd_ptr, write_fd_ptr);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_fork<R: SyscallRuntime>(runtime: &mut R) -> SyscallResult {
    let r = runtime.fork_child();
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_waitpid<R: SyscallRuntime>(runtime: &mut R, pid: u64) -> SyscallResult {
    // waitpid_impl either returns immediately (child already dead) or
    // diverges via exit_to_kernel (blocks the calling task).
    let r = runtime.waitpid_impl(pid);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_brk<R: SyscallRuntime>(runtime: &mut R, new_end: u64) -> SyscallResult {
    let r = runtime.brk_program(new_end);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_kill<R: SyscallRuntime>(runtime: &mut R, pid: u64) -> SyscallResult {
    let r = runtime.kill_pid(pid);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_readdir<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64, buf_ptr: u64, buf_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(path_ptr, path_len) { return SyscallResult::err(e); }
    if let Err(e) = validate_user_range(buf_ptr,  buf_len)  { return SyscallResult::err(e); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let buf  = unsafe { slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len as usize) };
    let r = runtime.readdir_impl(path, buf);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_dup2<R: SyscallRuntime>(runtime: &mut R, old_fd: i32, new_fd: i32) -> SyscallResult {
    let r = runtime.dup2_impl(old_fd, new_fd);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}
