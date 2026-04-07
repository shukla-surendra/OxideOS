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
    Invalid       = u64::MAX,
}

impl Syscall {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exit          => "exit",
            Self::Fork          => "fork",
            Self::Wait          => "wait",
            Self::GetPid        => "getpid",
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

    // ── Filesystem hooks (default: not supported) ──────────────────────────
    /// Open a file; `path` is raw bytes from user space.
    fn fs_open(&mut self, path: &[u8], flags: u32) -> i64 { ENOSYS }
    /// Close a file descriptor.
    fn fs_close(&mut self, fd: i32) -> i64 { ENOSYS }
    /// Read from a file descriptor into `buf`.
    fn fs_read(&mut self, fd: i32, buf: &mut [u8]) -> i64 { ENOSYS }
    /// Write `buf` to a file descriptor ≥ 3 (FD 1/2 go to `write_console`).
    fn fs_write_file(&mut self, fd: i32, buf: &[u8]) -> i64 { ENOSYS }
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
        Syscall::Fork          => SyscallResult::err(ENOSYS),
        Syscall::Wait          => SyscallResult::err(ENOSYS),
        Syscall::GetPid        => SyscallResult::ok(runtime.current_pid() as i64),
        Syscall::Mmap          => SyscallResult::err(ENOSYS),
        Syscall::Munmap        => SyscallResult::err(ENOSYS),
        Syscall::Brk           => SyscallResult::err(ENOSYS),
        Syscall::Read          => unsafe { sys_read(runtime, request.arg1 as i32,
                                                    request.arg2, request.arg3) },
        Syscall::Write         => unsafe { sys_write(runtime, request.arg1 as i32,
                                                     request.arg2, request.arg3) },
        Syscall::Open          => unsafe { sys_open(runtime, request.arg1,
                                                    request.arg2, request.arg3) },
        Syscall::Close         => sys_close(runtime, request.arg1 as i32),
        Syscall::Print         => unsafe { sys_print(runtime, request.arg1, request.arg2) },
        Syscall::GetChar       => SyscallResult::err(ENOSYS),
        Syscall::GetTime       => SyscallResult::ok(runtime.current_ticks() as i64),
        Syscall::Sleep         => sys_sleep(runtime, request.arg1),
        Syscall::GetSystemInfo => unsafe { sys_get_system_info(runtime, request.arg1) },
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
    // stdin (fd 0) not yet implemented
    if fd == 0 { return SyscallResult::err(ENOSYS); }
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
