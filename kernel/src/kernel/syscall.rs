// src/kernel/syscall.rs
//! System Call Interface for OxideOS
//! 
//! Provides the interface between user-space programs and kernel services
//! Uses the `syscall` instruction on x86_64

use crate::kernel::serial::SERIAL_PORT;
use core::arch::asm;

// ============================================================================
// SYSTEM CALL NUMBERS
// ============================================================================

#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Syscall {
    // Process Management
    Exit = 0,
    Fork = 1,
    Wait = 2,
    GetPid = 3,
    
    // Memory Management
    Mmap = 9,
    Munmap = 10,
    Brk = 11,
    
    // File Operations
    Read = 20,
    Write = 21,
    Open = 22,
    Close = 23,
    
    // Console/Terminal
    Print = 30,
    GetChar = 31,
    
    // Time
    GetTime = 40,
    Sleep = 41,
    
    // System Info
    GetSystemInfo = 50,
    
    // Invalid
    Invalid = 0xFFFFFFFF,
}

impl From<u64> for Syscall {
    fn from(num: u64) -> Self {
        match num {
            0 => Syscall::Exit,
            1 => Syscall::Fork,
            2 => Syscall::Wait,
            3 => Syscall::GetPid,
            9 => Syscall::Mmap,
            10 => Syscall::Munmap,
            11 => Syscall::Brk,
            20 => Syscall::Read,
            21 => Syscall::Write,
            22 => Syscall::Open,
            23 => Syscall::Close,
            30 => Syscall::Print,
            31 => Syscall::GetChar,
            40 => Syscall::GetTime,
            41 => Syscall::Sleep,
            50 => Syscall::GetSystemInfo,
            _ => Syscall::Invalid,
        }
    }
}

// ============================================================================
// SYSTEM CALL RESULT
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallResult {
    pub value: i64,
    pub error: bool,
}

impl SyscallResult {
    pub fn ok(value: i64) -> Self {
        Self { value, error: false }
    }
    
    pub fn err(error_code: i64) -> Self {
        Self { value: error_code, error: true }
    }
}

// ============================================================================
// ERROR CODES
// ============================================================================

pub const EINVAL: i64 = -1;  // Invalid argument
pub const ENOSYS: i64 = -2;  // Function not implemented
pub const EACCES: i64 = -3;  // Permission denied
pub const ENOMEM: i64 = -4;  // Out of memory
pub const EBADF: i64 = -5;   // Bad file descriptor
pub const EAGAIN: i64 = -6;  // Try again

// ============================================================================
// SYSTEM CALL DISPATCHER
// ============================================================================

/// Main system call handler - called from interrupt handler
pub unsafe fn handle_syscall(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> SyscallResult {
    let syscall = Syscall::from(syscall_num);
    
    SERIAL_PORT.write_str("SYSCALL: ");
    SERIAL_PORT.write_str(match syscall {
        Syscall::Exit => "Exit",
        Syscall::Print => "Print",
        Syscall::GetPid => "GetPid",
        Syscall::GetTime => "GetTime",
        Syscall::Write => "Write",
        Syscall::Read => "Read",
        _ => "Other",
    });
    SERIAL_PORT.write_str("\n");
    
    match syscall {
        Syscall::Exit => sys_exit(arg1 as i32),
        Syscall::Fork => sys_fork(),
        Syscall::Wait => sys_wait(arg1),
        Syscall::GetPid => sys_getpid(),
        
        Syscall::Mmap => sys_mmap(arg1, arg2, arg3, arg4, arg5),
        Syscall::Munmap => sys_munmap(arg1, arg2),
        Syscall::Brk => sys_brk(arg1),
        
        Syscall::Read => sys_read(arg1 as i32, arg2, arg3),
        Syscall::Write => sys_write(arg1 as i32, arg2, arg3),
        Syscall::Open => sys_open(arg1, arg2, arg3),
        Syscall::Close => sys_close(arg1 as i32),
        
        Syscall::Print => sys_print(arg1, arg2),
        Syscall::GetChar => sys_getchar(),
        
        Syscall::GetTime => sys_gettime(),
        Syscall::Sleep => sys_sleep(arg1),
        
        Syscall::GetSystemInfo => sys_get_system_info(arg1),
        
        Syscall::Invalid => SyscallResult::err(ENOSYS),
    }
}

// ============================================================================
// SYSTEM CALL IMPLEMENTATIONS
// ============================================================================

// Process Management
// ------------------

unsafe fn sys_exit(code: i32) -> SyscallResult {
    SERIAL_PORT.write_str("Process exiting with code: ");
    SERIAL_PORT.write_decimal(code as u32);
    SERIAL_PORT.write_str("\n");
    
    // TODO: Actually terminate the process
    // For now, just halt
    loop { asm!("hlt") }
}

unsafe fn sys_fork() -> SyscallResult {
    SERIAL_PORT.write_str("Fork not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

unsafe fn sys_wait(_pid: u64) -> SyscallResult {
    SERIAL_PORT.write_str("Wait not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

unsafe fn sys_getpid() -> SyscallResult {
    // TODO: Return actual PID from process manager
    SyscallResult::ok(1) // Temporary: always return PID 1
}

// Memory Management
// -----------------

unsafe fn sys_mmap(_addr: u64, _length: u64, _prot: u64, _flags: u64, _fd: u64) -> SyscallResult {
    SERIAL_PORT.write_str("Mmap not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

unsafe fn sys_munmap(_addr: u64, _length: u64) -> SyscallResult {
    SERIAL_PORT.write_str("Munmap not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

unsafe fn sys_brk(_addr: u64) -> SyscallResult {
    SERIAL_PORT.write_str("Brk not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

// File Operations
// ---------------

unsafe fn sys_read(_fd: i32, _buf: u64, _count: u64) -> SyscallResult {
    SERIAL_PORT.write_str("Read not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

unsafe fn sys_write(fd: i32, buf_ptr: u64, count: u64) -> SyscallResult {
    // Only support stdout (fd=1) and stderr (fd=2) for now
    if fd != 1 && fd != 2 {
        return SyscallResult::err(EBADF);
    }
    
    // Validate pointer is in user space
    if buf_ptr < 0x1000 || buf_ptr >= 0xFFFF800000000000 {
        return SyscallResult::err(EINVAL);
    }
    
    let buf = core::slice::from_raw_parts(buf_ptr as *const u8, count as usize);
    
    // Write to serial port
    for &byte in buf {
        SERIAL_PORT.write_byte(byte);
    }
    
    SyscallResult::ok(count as i64)
}

unsafe fn sys_open(_path: u64, _flags: u64, _mode: u64) -> SyscallResult {
    SERIAL_PORT.write_str("Open not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

unsafe fn sys_close(_fd: i32) -> SyscallResult {
    SERIAL_PORT.write_str("Close not yet implemented\n");
    SyscallResult::err(ENOSYS)
}

// Console Operations
// ------------------

unsafe fn sys_print(msg_ptr: u64, len: u64) -> SyscallResult {
    // Validate pointer
    if msg_ptr < 0x1000 || msg_ptr >= 0xFFFF800000000000 {
        return SyscallResult::err(EINVAL);
    }
    
    let msg = core::slice::from_raw_parts(msg_ptr as *const u8, len as usize);
    
    SERIAL_PORT.write_str("USER: ");
    for &byte in msg {
        SERIAL_PORT.write_byte(byte);
    }
    SERIAL_PORT.write_str("\n");
    
    SyscallResult::ok(len as i64)
}

unsafe fn sys_getchar() -> SyscallResult {
    // TODO: Read from keyboard buffer
    SyscallResult::err(ENOSYS)
}

// Time Operations
// ---------------

unsafe fn sys_gettime() -> SyscallResult {
    use crate::kernel::timer;
    let ticks = timer::get_ticks();
    SyscallResult::ok(ticks as i64)
}

unsafe fn sys_sleep(ms: u64) -> SyscallResult {
    use crate::kernel::timer;
    let start = timer::get_ticks();
    let target = start + (ms * 100 / 1000); // Convert ms to ticks (100 Hz timer)
    
    while timer::get_ticks() < target {
        asm!("hlt");
    }
    
    SyscallResult::ok(0)
}

// System Information
// ------------------

#[repr(C)]
pub struct SystemInfo {
    pub total_memory: u64,
    pub free_memory: u64,
    pub uptime_ms: u64,
    pub process_count: u32,
}

unsafe fn sys_get_system_info(info_ptr: u64) -> SyscallResult {
    if info_ptr < 0x1000 || info_ptr >= 0xFFFF800000000000 {
        return SyscallResult::err(EINVAL);
    }
    
    use crate::kernel::timer;
    
    let info = &mut *(info_ptr as *mut SystemInfo);
    info.total_memory = 128 * 1024 * 1024; // 128 MB - placeholder
    info.free_memory = 64 * 1024 * 1024;   // 64 MB - placeholder
    info.uptime_ms = (timer::get_ticks() * 1000 / 100) as u64; // Convert ticks to ms
    info.process_count = 1; // Placeholder
    
    SyscallResult::ok(0)
}

// ============================================================================
// USER-SPACE SYSCALL WRAPPERS (for testing)
// ============================================================================

#[cfg(feature = "user_syscalls")]
pub mod user {
    use super::*;
    
    /// Make a system call from user space
    /// Returns: result in RAX
    #[inline]
    pub unsafe fn syscall0(num: u64) -> i64 {
        let ret: i64;
        asm!(
            "syscall",
            inlateout("rax") num => ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
        ret
    }
    
    #[inline]
    pub unsafe fn syscall1(num: u64, arg1: u64) -> i64 {
        let ret: i64;
        asm!(
            "syscall",
            inlateout("rax") num => ret,
            in("rdi") arg1,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
        ret
    }
    
    #[inline]
    pub unsafe fn syscall2(num: u64, arg1: u64, arg2: u64) -> i64 {
        let ret: i64;
        asm!(
            "syscall",
            inlateout("rax") num => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
        ret
    }
    
    #[inline]
    pub unsafe fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
        let ret: i64;
        asm!(
            "syscall",
            inlateout("rax") num => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
        ret
    }
    
    // High-level wrappers
    
    pub fn exit(code: i32) -> ! {
        unsafe {
            syscall1(Syscall::Exit as u64, code as u64);
            loop { asm!("hlt") }
        }
    }
    
    pub fn getpid() -> i32 {
        unsafe { syscall0(Syscall::GetPid as u64) as i32 }
    }
    
    pub fn print(msg: &str) -> isize {
        unsafe {
            syscall2(
                Syscall::Print as u64,
                msg.as_ptr() as u64,
                msg.len() as u64
            ) as isize
        }
    }
    
    pub fn write(fd: i32, buf: &[u8]) -> isize {
        unsafe {
            syscall3(
                Syscall::Write as u64,
                fd as u64,
                buf.as_ptr() as u64,
                buf.len() as u64
            ) as isize
        }
    }
    
    pub fn gettime() -> u64 {
        unsafe { syscall0(Syscall::GetTime as u64) as u64 }
    }
    
    pub fn sleep(ms: u64) {
        unsafe { syscall1(Syscall::Sleep as u64, ms) };
    }
    
    pub fn get_system_info() -> SystemInfo {
        let mut info = SystemInfo {
            total_memory: 0,
            free_memory: 0,
            uptime_ms: 0,
            process_count: 0,
        };
        
        unsafe {
            syscall1(Syscall::GetSystemInfo as u64, &mut info as *mut _ as u64);
        }
        
        info
    }
}