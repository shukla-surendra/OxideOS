//! Kernel-facing syscall adapter for OxideOS.
//!
//! The actual syscall rules live in [`super::syscall_core`]. This file wires
//! those rules to live kernel services such as the serial port and timer.

use crate::kernel::serial::SERIAL_PORT;
use core::arch::asm;

pub use super::syscall_core::{Syscall, SyscallRequest, SyscallResult, SystemInfo};
use super::syscall_core::{dispatch, SyscallRuntime};

struct KernelRuntime;

impl SyscallRuntime for KernelRuntime {
    fn trace(&mut self, syscall: Syscall) {
        unsafe {
            SERIAL_PORT.write_str("SYSCALL: ");
            SERIAL_PORT.write_str(syscall.name());
            SERIAL_PORT.write_str("\n");
        }
    }

    fn current_ticks(&self) -> u64 {
        unsafe { crate::kernel::timer::get_ticks() }
    }

    fn write_console(&mut self, bytes: &[u8]) {
        unsafe {
            for &byte in bytes {
                SERIAL_PORT.write_byte(byte);
            }
        }
    }

    fn fill_system_info(&self, info: &mut SystemInfo) {
        let ticks = unsafe { crate::kernel::timer::get_ticks() };
        *info = SystemInfo {
            total_memory: 128 * 1024 * 1024,
            free_memory: 64 * 1024 * 1024,
            uptime_ms: ticks * 1000 / super::syscall_core::TIMER_HZ,
            process_count: 1,
        };
    }

    fn sleep_until_tick(&mut self, target_tick: u64) {
        while unsafe { crate::kernel::timer::get_ticks() } < target_tick {
            unsafe { asm!("hlt"); }
        }
    }

    fn exit(&mut self, code: i32) -> ! {
        if crate::kernel::user_mode::is_active() {
            unsafe {
                SERIAL_PORT.write_str("User task exiting with code: ");
                SERIAL_PORT.write_decimal(code as u32);
                SERIAL_PORT.write_str("\n");
                crate::kernel::user_mode::exit_to_kernel(code as i64);
            }
        }
        unsafe {
            SERIAL_PORT.write_str("Process exiting with code: ");
            SERIAL_PORT.write_decimal(code as u32);
            SERIAL_PORT.write_str("\n");
        }
        loop { unsafe { asm!("hlt"); } }
    }

    // ── Filesystem ──────────────────────────────────────────────────────────

    fn fs_open(&mut self, path: &[u8], flags: u32) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -1,
        };
        unsafe {
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => fs.open(path_str, flags),
                None     => -2,
            }
        }
    }

    fn fs_close(&mut self, fd: i32) -> i64 {
        unsafe {
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => fs.close(fd),
                None     => -2,
            }
        }
    }

    fn fs_read(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        unsafe {
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => fs.read_fd(fd, buf),
                None     => -2,
            }
        }
    }

    fn fs_write_file(&mut self, fd: i32, buf: &[u8]) -> i64 {
        unsafe {
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => fs.write_fd(fd, buf),
                None     => -2,
            }
        }
    }
}

/// Main system call entry used by the interrupt dispatcher.
pub unsafe fn handle_syscall(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> SyscallResult {
    let mut runtime = KernelRuntime;
    dispatch(
        &mut runtime,
        SyscallRequest::new(syscall_num, arg1, arg2, arg3, arg4, arg5),
    )
}

/// Lightweight boot-time smoke tests for the dispatcher.
///
/// Pointer-carrying syscalls are covered by host-side tests because kernel
/// pointers live in the higher half and should be rejected by user-pointer
/// validation.
pub unsafe fn run_boot_self_tests() {
    SERIAL_PORT.write_str("\n=== SYSCALL BOOT SELF-TESTS ===\n");

    let pid = handle_syscall(Syscall::GetPid as u64, 0, 0, 0, 0, 0);
    SERIAL_PORT.write_str("  getpid -> ");
    SERIAL_PORT.write_decimal(pid.value as u32);
    SERIAL_PORT.write_str("\n");

    let ticks = handle_syscall(Syscall::GetTime as u64, 0, 0, 0, 0, 0);
    SERIAL_PORT.write_str("  gettime -> ");
    SERIAL_PORT.write_decimal(ticks.value as u32);
    SERIAL_PORT.write_str(" ticks\n");

    let unsupported = handle_syscall(0xFFFF, 0, 0, 0, 0, 0);
    SERIAL_PORT.write_str("  invalid syscall -> ");
    SERIAL_PORT.write_decimal(unsupported.value as u32);
    SERIAL_PORT.write_str("\n");

    SERIAL_PORT.write_str("=== SYSCALL BOOT SELF-TESTS COMPLETE ===\n\n");
}

pub fn snapshot_system_info() -> SystemInfo {
    let ticks = unsafe { crate::kernel::timer::get_ticks() };
    SystemInfo {
        total_memory: 128 * 1024 * 1024,
        free_memory: 64 * 1024 * 1024,
        uptime_ms: ticks * 1000 / super::syscall_core::TIMER_HZ,
        process_count: 1,
    }
}

#[cfg(feature = "user_syscalls")]
pub mod user {
    use super::*;

    /// Make a system call from user space using the fast `syscall` instruction.
    ///
    /// This is intentionally feature-gated because OxideOS does not yet have
    /// the full user-mode/TSS stack switching work needed to make this path
    /// production-ready.
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

    pub fn exit(code: i32) -> ! {
        unsafe {
            syscall1(Syscall::Exit as u64, code as u64);
            loop {
                asm!("hlt");
            }
        }
    }

    pub fn getpid() -> i32 {
        unsafe { syscall0(Syscall::GetPid as u64) as i32 }
    }

    pub fn print(msg: &str) -> isize {
        unsafe { syscall2(Syscall::Print as u64, msg.as_ptr() as u64, msg.len() as u64) as isize }
    }

    pub fn write(fd: i32, buf: &[u8]) -> isize {
        unsafe {
            syscall3(
                Syscall::Write as u64,
                fd as u64,
                buf.as_ptr() as u64,
                buf.len() as u64,
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
        let mut info = SystemInfo::default();

        unsafe {
            syscall1(Syscall::GetSystemInfo as u64, &mut info as *mut _ as u64);
        }

        info
    }
}
