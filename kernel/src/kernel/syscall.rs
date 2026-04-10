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
        // Always echo to serial for debugging.
        unsafe {
            for &byte in bytes {
                SERIAL_PORT.write_byte(byte);
            }
        }
        // While a user program is running, also capture output for the GUI terminal.
        if crate::kernel::user_mode::is_active() {
            crate::kernel::user_mode::output_write(bytes);
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
        unsafe {
            // If the scheduler is managing the calling task, yield so the
            // kernel can render GUI frames while the task sleeps.
            let ctx_opt = core::ptr::replace(
                &raw mut crate::kernel::user_mode::CURRENT_SYSCALL_CTX,
                None,
            );
            if let Some(ctx) = ctx_opt {
                if crate::kernel::scheduler::has_task() {
                    crate::kernel::scheduler::sleep_task(target_tick, ctx);
                    // ^^^ noreturn — resumes in ring-3 after sleep expires
                }
            }
            // Fallback: busy-wait (used during boot before scheduler is live).
            while crate::kernel::timer::get_ticks() < target_tick {
                asm!("hlt");
            }
        }
    }

    fn get_char(&mut self) -> i64 {
        match crate::kernel::stdin::pop() {
            Some(ch) => ch as i64,
            None     => -6, // EAGAIN
        }
    }

    fn exit(&mut self, code: i32) -> ! {
        if crate::kernel::user_mode::is_active() {
            unsafe {
                SERIAL_PORT.write_str("User task exiting (code ");
                SERIAL_PORT.write_decimal(code as u32);
                SERIAL_PORT.write_str(")\n");
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

    fn current_pid(&self) -> u64 {
        unsafe {
            let sched = &raw const crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            (*sched).tasks[idx].pid as u64
        }
    }

    fn exec_program(&mut self, path: &[u8]) -> i64 {
        self.exec_resolve(path)
    }

    fn fork_child(&mut self) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::user_mode::CURRENT_SYSCALL_CTX;

            let parent_idx = CURRENT_TASK_IDX;

            // Capture the parent's context — child will resume at the same RIP
            // (instruction after int 0x80) but with rax = 0.
            let mut child_ctx = match CURRENT_SYSCALL_CTX {
                Some(ctx) => ctx,
                None      => return -1,
            };
            child_ctx.rax = 0; // child returns 0 from fork

            match crate::kernel::scheduler::fork_task(parent_idx, child_ctx) {
                Ok(child_pid) => child_pid as i64,
                Err(_)        => -4, // ENOMEM
            }
        }
    }

    fn waitpid_impl(&mut self, pid: u64) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, MAX_TASKS, TaskState};
            use crate::kernel::user_mode::CURRENT_SYSCALL_CTX;

            let target_pid = pid as u8;
            let parent_idx = CURRENT_TASK_IDX;
            let parent_pid = (*(&raw const SCHED)).tasks[parent_idx].pid;

            // Check if the child is already dead.
            let sched = &raw mut SCHED;
            for i in 0..MAX_TASKS {
                if (*sched).tasks[i].pid        == target_pid
                && (*sched).tasks[i].parent_pid == parent_pid
                {
                    if let TaskState::Dead(code) = (*sched).tasks[i].state {
                        (*sched).tasks[i].state   = TaskState::Empty;
                        (*sched).tasks[i].pid     = 0;
                        (*sched).tasks[i].parent_pid = 0;
                        return code;
                    }
                    // Child exists but still alive — fall through to block.
                    let ctx_opt = core::ptr::replace(&raw mut CURRENT_SYSCALL_CTX, None);
                    if let Some(ctx) = ctx_opt {
                        crate::kernel::scheduler::wait_for_pid(parent_idx, target_pid, ctx);
                        // ^^^ diverges — resumes in ring-3 when child dies
                    }
                    return -6; // EAGAIN (only if context was not available)
                }
            }
            -3 // EACCES — no such child of this process
        }
    }

    fn brk_program(&mut self, new_end: u64) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::paging_allocator as pa;

            const USER_HEAP_BASE: u64 = 0x0100_0000; // 16 MB — well above code+stack
            const PAGE_SIZE:      u64 = 4096;

            let sched   = &raw mut SCHED;
            let idx     = CURRENT_TASK_IDX;
            let cr3     = (*sched).tasks[idx].cr3;
            let cur_end = {
                let h = (*sched).tasks[idx].heap_end;
                if h == 0 { USER_HEAP_BASE } else { h }
            };

            // brk(0) — query current break.
            if new_end == 0 {
                return cur_end as i64;
            }
            // Refuse to shrink below heap base or to move backwards (keep it simple).
            if new_end < USER_HEAP_BASE || new_end <= cur_end {
                return cur_end as i64;
            }

            // Map new pages from cur_end up to new_end.
            let first_page = (cur_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
            let last_page  = (new_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
            if last_page > first_page {
                let npages = ((last_page - first_page) / PAGE_SIZE) as usize;
                if pa::map_user_region_in(cr3, first_page, npages, true, false).is_err() {
                    return -4; // ENOMEM
                }
            }

            (*sched).tasks[idx].heap_end = new_end;
            new_end as i64
        }
    }

    fn kill_pid(&mut self, pid: u64) -> i64 {
        let ok = unsafe { crate::kernel::scheduler::kill(pid as u8) };
        if ok { 0 } else { -3 }
    }

    fn readdir_impl(&mut self, path: &[u8], buf: &mut [u8]) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22, // EINVAL
        };
        crate::kernel::vfs::vfs_readdir(path_str, buf)
    }

    fn dup2_impl(&mut self, old_fd: i32, new_fd: i32) -> i64 {
        unsafe {
            let sched = &raw mut crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            (*sched).tasks[idx].fd_table.dup2(old_fd, new_fd)
        }
    }

    fn fs_open(&mut self, path: &[u8], flags: u32) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22, // EINVAL
        };
        unsafe { crate::kernel::vfs::vfs_open(path_str, flags) }
    }

    fn fs_close(&mut self, fd: i32) -> i64 {
        // FdTable::close dispatches to the right backend (pipe/FAT16/RamFS/dev).
        unsafe {
            let sched = &raw mut crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            (*sched).tasks[idx].fd_table.close(fd)
        }
    }

    fn fs_read(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        // FdTable::read_fd handles all backends including pipes and FAT16.
        unsafe {
            let sched = &raw mut crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            let fdt   = &raw mut (*sched).tasks[idx].fd_table;
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => (*fdt).read_fd(fs, fd, buf),
                None     => -2,
            }
        }
    }

    fn fs_write_file(&mut self, fd: i32, buf: &[u8]) -> i64 {
        // FdTable::write_fd handles all backends.
        // For fd=1/2 with no FdTable entry, returns EBADF; caller falls back to console.
        unsafe {
            let sched = &raw mut crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            let fdt   = &raw mut (*sched).tasks[idx].fd_table;
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => (*fdt).write_fd(fs, fd, buf),
                None     => -2,
            }
        }
    }

    fn pipe_alloc(&mut self, read_fd_ptr: u64, write_fd_ptr: u64) -> i64 {
        unsafe {
            let (raw_r, raw_w) = match crate::kernel::pipe::alloc() {
                Some(pair) => pair,
                None       => return -6, // EAGAIN: out of raw pipes
            };
            let sched = &raw mut crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            let fdt   = &raw mut (*sched).tasks[idx].fd_table;
            match (*fdt).open_pipe(raw_r, raw_w) {
                Some((rslot, wslot)) => {
                    core::ptr::write_unaligned(read_fd_ptr  as *mut i32, rslot as i32);
                    core::ptr::write_unaligned(write_fd_ptr as *mut i32, wslot as i32);
                    0
                }
                None => {
                    // No room in FD table; release the raw pipe.
                    crate::kernel::pipe::close(raw_r);
                    crate::kernel::pipe::close(raw_w);
                    -24 // EMFILE
                }
            }
        }
    }

    fn msgq_create(&mut self, id: u32) -> i64 {
        unsafe { crate::kernel::ipc::msgq_create(id) }
    }

    fn msgsnd(&mut self, id: u32, type_id: u32, data: &[u8]) -> i64 {
        unsafe { crate::kernel::ipc::msgsnd(id, type_id, data) }
    }

    fn msgrcv(&mut self, id: u32, msg_out_ptr: u64) -> i64 {
        unsafe {
            let mut msg = crate::kernel::ipc::Message::empty();
            let res = crate::kernel::ipc::msgrcv(id, &mut msg);
            if res == 0 {
                core::ptr::write_unaligned(msg_out_ptr as *mut crate::kernel::ipc::Message, msg);
            }
            res
        }
    }

    fn msgq_destroy(&mut self, id: u32) -> i64 {
        unsafe { crate::kernel::ipc::msgq_destroy(id) }
    }

    /// Blocking receive.  If the queue is empty the task is suspended via the
    /// scheduler (same mechanism as Sleep / Waitpid).  `tick()` will dequeue
    /// the message and wake the task on the next frame that has data.
    fn msgrcv_wait(&mut self, id: u32, msg_out_ptr: u64) -> i64 {
        unsafe {
            // Fast path: queue already has data — dequeue immediately.
            let mut msg = crate::kernel::ipc::Message::empty();
            if crate::kernel::ipc::msgrcv(id, &mut msg) == 0 {
                core::ptr::write_unaligned(msg_out_ptr as *mut crate::kernel::ipc::Message, msg);
                return 0;
            }

            // Slow path: block the task until a message arrives.
            let ctx_opt = core::ptr::replace(
                &raw mut crate::kernel::user_mode::CURRENT_SYSCALL_CTX,
                None,
            );
            if let Some(ctx) = ctx_opt {
                // Diverges — control returns to the GUI main loop.
                crate::kernel::scheduler::wait_for_msg(id, msg_out_ptr, ctx);
            }
            // Fallback if no scheduler context (should not happen in normal use).
            -11 // EAGAIN with no scheduler
        }
    }

    fn msgq_len(&mut self, id: u32) -> i64 {
        unsafe { crate::kernel::ipc::msgq_len(id) }
    }
}

// ── exec helpers (not part of the trait; called via exec_program) ─────────

impl KernelRuntime {
    /// Resolve path → binary bytes, then hand off to `exec_binary`.
    fn exec_resolve(&mut self, path: &[u8]) -> i64 {
        extern crate alloc;
        use alloc::vec::Vec;

        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -1,
        };

        // 1. Built-in registry (embedded binaries — no disk needed).
        let short = path_str.trim_start_matches('/').trim_start_matches("bin/");
        if let Some(b) = crate::kernel::programs::find(short) {
            return self.exec_binary(b);
        }

        // 2. RamFS
        if let Some(data) = unsafe { crate::kernel::fs::ramfs::RAMFS.get() }
            .and_then(|fs| fs.read_file(path_str))
        {
            if !data.is_empty() {
                let owned: Vec<u8> = data.to_vec();
                return self.exec_binary(&owned);
            }
        }

        // 3. FAT16
        if path.starts_with(b"/disk/") {
            return self.exec_fat(path);
        }

        -2 // ENOENT
    }

    fn exec_fat(&mut self, path: &[u8]) -> i64 {
        extern crate alloc;
        use alloc::vec::Vec;
        let fd = unsafe { crate::kernel::fat::open(path, 0) };
        if fd < 0 { return -2; }
        let mut buf: Vec<u8> = Vec::new();
        let mut tmp = [0u8; 512];
        loop {
            let n = unsafe { crate::kernel::fat::read_fd(fd as i32, &mut tmp) };
            if n <= 0 { break; }
            buf.extend_from_slice(&tmp[..n as usize]);
        }
        let _ = unsafe { crate::kernel::fat::close(fd as i32) };
        if buf.is_empty() { return -2; }
        self.exec_binary(&buf)
    }

    /// Load `binary` into a fresh address space and replace the current task.
    /// On success this never returns; on failure returns a negative error code.
    fn exec_binary(&mut self, binary: &[u8]) -> i64 {
        use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, EXIT_PREEMPTED};
        use crate::kernel::paging_allocator as pa;
        use crate::kernel::fs::ramfs::FdTable;

        const PAGE_SIZE:        usize = 4096;
        const USER_STACK_TOP:   u64   = 0x0080_0000;
        const USER_STACK_PAGES: usize = 4;
        const USER_CODE_ADDR:   u64   = 0x0040_0000;
        let stack_base = USER_STACK_TOP - (USER_STACK_PAGES * PAGE_SIZE) as u64;

        // Create a fresh page table.
        let new_cr3 = match unsafe { pa::create_user_page_table() } {
            Some(cr3) => cr3,
            None      => return -4,
        };

        // Map user stack.
        if unsafe { pa::map_user_region_in(new_cr3, stack_base, USER_STACK_PAGES, true, false) }.is_err() {
            return -4;
        }

        // Load ELF or flat binary into the new CR3.
        let entry = if crate::kernel::elf_loader::is_elf(binary) {
            match unsafe { crate::kernel::elf_loader::load_in(binary, new_cr3) } {
                Ok(e)  => e,
                Err(_) => return -1,
            }
        } else {
            let npages = binary.len().div_ceil(PAGE_SIZE);
            if unsafe { pa::map_user_region_in(new_cr3, USER_CODE_ADDR, npages, true, true) }.is_err() {
                return -4;
            }
            unsafe { pa::copy_to_region_in(new_cr3, USER_CODE_ADDR, binary); }
            USER_CODE_ADDR
        };

        // Capture old CR3 before overwriting.
        let old_cr3 = unsafe {
            let s = &raw const SCHED;
            (*s).tasks[CURRENT_TASK_IDX].cr3
        };

        // Update current task: new image, reset FD table but inherit stdin/stdout/stderr.
        unsafe {
            let s    = &raw mut SCHED;
            let idx  = CURRENT_TASK_IDX;
            let task = &raw mut (*s).tasks[idx];
            // Save fd 0/1/2 before wiping the table (Unix exec inherits these).
            let saved_std = [
                (*task).fd_table.entries[0],
                (*task).fd_table.entries[1],
                (*task).fd_table.entries[2],
            ];
            (*task).cr3        = new_cr3;
            (*task).entry      = entry;
            (*task).first_run  = true;
            (*task).fd_table   = FdTable::new();
            (*task).fd_table.entries[0] = saved_std[0];
            (*task).fd_table.entries[1] = saved_std[1];
            (*task).fd_table.entries[2] = saved_std[2];
            (*task).output_len = 0;
        }

        // Free old page table (user half only; kernel half is shared).
        if old_cr3 != 0 {
            unsafe { pa::free_user_page_table(old_cr3); }
        }

        // Non-local goto back to tick().  tick() will see EXIT_PREEMPTED,
        // mark the task Ready, and on the next tick launch_at(entry, stack, new_cr3).
        unsafe {
            crate::kernel::user_mode::CURRENT_SYSCALL_CTX = None;
            crate::kernel::user_mode::exit_to_kernel(EXIT_PREEMPTED);
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
