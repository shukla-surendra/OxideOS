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
        self.exec_resolve(path, "")
    }

    fn exec_program_args(&mut self, path: &[u8], args: &[u8]) -> i64 {
        let args_str = core::str::from_utf8(args).unwrap_or("");
        self.exec_resolve(path, args_str)
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

    fn mmap_anon(&mut self, _hint: u64, len: u64) -> i64 {
        if len == 0 { return -22; } // EINVAL
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, MmapRegion, MAX_MMAP_REGIONS};
            use crate::kernel::paging_allocator as pa;

            const USER_MMAP_BASE: u64 = 0x0800_0000;
            const PAGE_SIZE: u64 = 4096;

            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let cr3   = (*sched).tasks[idx].cr3;

            let base = {
                let end = (*sched).tasks[idx].mmap_end;
                if end == 0 { USER_MMAP_BASE } else { end }
            };

            let pages = ((len + PAGE_SIZE - 1) / PAGE_SIZE) as usize;

            if pa::map_user_region_in(cr3, base, pages, true, false).is_err() {
                return -12; // ENOMEM
            }

            let new_end = base + pages as u64 * PAGE_SIZE;
            (*sched).tasks[idx].mmap_end = new_end;

            // Record this region for munmap.
            let nr = (*sched).tasks[idx].mmap_nregions;
            if nr < MAX_MMAP_REGIONS {
                (*sched).tasks[idx].mmap_regions[nr] = MmapRegion { virt: base, pages: pages as u32, _pad: 0 };
                (*sched).tasks[idx].mmap_nregions = nr + 1;
            }

            base as i64
        }
    }

    fn munmap_impl(&mut self, addr: u64, len: u64) -> i64 {
        if len == 0 { return -22; } // EINVAL
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, MmapRegion, MAX_MMAP_REGIONS};
            use crate::kernel::paging_allocator as pa;

            const PAGE_SIZE: u64 = 4096;

            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let cr3   = (*sched).tasks[idx].cr3;
            let pages = ((len + PAGE_SIZE - 1) / PAGE_SIZE) as usize;

            pa::unmap_user_region_in(cr3, addr, pages);

            // Remove from the tracked region list (swap-remove).
            let nr = (*sched).tasks[idx].mmap_nregions;
            for i in 0..nr {
                if (*sched).tasks[idx].mmap_regions[i].virt == addr {
                    (*sched).tasks[idx].mmap_regions[i] = (*sched).tasks[idx].mmap_regions[nr - 1];
                    (*sched).tasks[idx].mmap_regions[nr - 1] = MmapRegion::empty();
                    (*sched).tasks[idx].mmap_nregions = nr - 1;
                    break;
                }
            }
            0
        }
    }

    fn kill_pid_sig(&mut self, pid: u64, signum: u8) -> i64 {
        let ok = unsafe { crate::kernel::scheduler::send_signal(pid as u8, signum) };
        if ok { 0 } else { -3 }
    }

    fn getppid_impl(&mut self) -> i64 {
        unsafe {
            let sched = &raw const crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            (*sched).tasks[idx].parent_pid as i64
        }
    }

    fn uname_impl(&mut self, buf_ptr: u64) -> i64 {
        // Linux utsname: 6 fields × 65 bytes each = 390 bytes.
        // Fields: sysname, nodename, release, version, machine, domainname.
        if buf_ptr == 0 { return -14; } // EFAULT
        let buf = buf_ptr as *mut u8;
        let field = 65usize;
        unsafe {
            let write_field = |offset: usize, s: &[u8]| {
                let dst = buf.add(offset);
                core::ptr::write_bytes(dst, 0, field);
                let n = s.len().min(field - 1);
                core::ptr::copy_nonoverlapping(s.as_ptr(), dst, n);
            };
            write_field(0,         b"OxideOS");
            write_field(field,     b"oxideos");
            write_field(field * 2, b"0.1.0");
            write_field(field * 3, b"#1 SMP");
            write_field(field * 4, b"x86_64");
            write_field(field * 5, b"(none)");
        }
        0
    }

    fn clock_gettime_impl(&mut self, _clk_id: u64, tp_ptr: u64) -> i64 {
        if tp_ptr == 0 { return -14; } // EFAULT
        // Return monotonic time from PIT ticks (100 Hz).
        let ticks = unsafe { crate::kernel::timer::get_ticks() };
        let secs  = ticks / 100;
        let nsecs = (ticks % 100) * 10_000_000;
        unsafe {
            core::ptr::write_unaligned(tp_ptr as *mut u64, secs);
            core::ptr::write_unaligned((tp_ptr + 8) as *mut u64, nsecs);
        }
        0
    }

    fn fcntl_impl(&mut self, fd: i32, cmd: u64, arg: u64) -> i64 {
        // F_GETFL=3 → return O_RDWR (2); F_SETFL=4 → return 0; others → 0.
        match cmd {
            1 => { // F_DUPFD: dup fd to first available ≥ arg
                unsafe {
                    use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
                    let sched = &raw mut SCHED;
                    let idx   = CURRENT_TASK_IDX;
                    let table = &mut (*sched).tasks[idx].fd_table;
                    let src = fd as usize;
                    if src >= crate::kernel::fs::ramfs::MAX_FD { return -9; }
                    let entry = match table.entries[src] { Some(e) => e, None => return -9 };
                    let start = arg as usize;
                    for new_fd in start..crate::kernel::fs::ramfs::MAX_FD {
                        if table.entries[new_fd].is_none() {
                            table.entries[new_fd] = Some(entry);
                            return new_fd as i64;
                        }
                    }
                    -24 // EMFILE
                }
            }
            2 => { // F_GETFD: return close-on-exec flag (always 0)
                0
            }
            3 => 2, // F_GETFL: O_RDWR
            4 => 0, // F_SETFL: ignore
            _ => 0,
        }
    }

    // ── Socket syscalls ────────────────────────────────────────────────────

    fn socket_impl(&mut self, domain: u32, type_: u32, proto: u32) -> i64 {
        unsafe { crate::kernel::net::socket::sys_socket(domain, type_, proto) }
    }

    unsafe fn bind_impl(&mut self, sfd: u64, addr_ptr: u64, addr_len: usize) -> i64 {
        unsafe { crate::kernel::net::socket::sys_bind(sfd as i64, addr_ptr as *const u8, addr_len) }
    }

    unsafe fn connect_impl(&mut self, sfd: u64, addr_ptr: u64, addr_len: usize) -> i64 {
        unsafe { crate::kernel::net::socket::sys_connect(sfd as i64, addr_ptr as *const u8, addr_len) }
    }

    fn listen_impl(&mut self, sfd: u64, backlog: i32) -> i64 {
        unsafe { crate::kernel::net::socket::sys_listen(sfd as i64, backlog) }
    }

    fn accept_impl(&mut self, sfd: u64) -> i64 {
        unsafe { crate::kernel::net::socket::sys_accept(sfd as i64) }
    }

    unsafe fn send_impl(&mut self, sfd: u64, buf_ptr: u64, len: usize, flags: u32) -> i64 {
        unsafe { crate::kernel::net::socket::sys_send(sfd as i64, buf_ptr as *const u8, len, flags) }
    }

    unsafe fn recv_impl(&mut self, sfd: u64, buf_ptr: u64, len: usize, flags: u32) -> i64 {
        unsafe { crate::kernel::net::socket::sys_recv(sfd as i64, buf_ptr as *mut u8, len, flags) }
    }

    fn close_socket_impl(&mut self, sfd: u64) -> i64 {
        unsafe { crate::kernel::net::socket::sys_close_socket(sfd as i64) }
    }

    unsafe fn sendto_impl(&mut self, sfd: u64, buf_ptr: u64, len: usize, flags: u32,
                          addr_ptr: u64, addr_len: usize) -> i64 {
        unsafe { crate::kernel::net::socket::sys_sendto(
            sfd as i64, buf_ptr as *const u8, len, flags,
            addr_ptr as *const u8, addr_len,
        )}
    }

    unsafe fn recvfrom_impl(&mut self, sfd: u64, buf_ptr: u64, len: usize, flags: u32,
                            addr_ptr: u64, addr_len_ptr: u64) -> i64 {
        unsafe { crate::kernel::net::socket::sys_recvfrom(
            sfd as i64, buf_ptr as *mut u8, len, flags,
            addr_ptr as *mut u8, addr_len_ptr as *mut u32,
        )}
    }

    fn getdents64_impl(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        // struct linux_dirent64: d_ino(8) d_off(8) d_reclen(2) d_type(1) d_name(n+1)
        // d_type: 4=DT_DIR, 8=DT_REG, 0=DT_UNKNOWN
        const DT_UNKNOWN: u8 = 0;
        const DT_DIR: u8 = 4;
        const DT_REG: u8 = 8;

        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::fs::ramfs::FdBackend;

            if fd < 0 || fd as usize >= crate::kernel::fs::ramfs::MAX_FD { return -9; }
            let sched = &raw mut SCHED;
            let idx = CURRENT_TASK_IDX;
            let entry = match (*sched).tasks[idx].fd_table.entries[fd as usize] {
                None => return -9,
                Some(e) => e,
            };

            if !matches!(entry.backend, FdBackend::Dir) { return -20; } // ENOTDIR

            // Get the directory path from the entry
            let path_len = entry.dir_path_len as usize;
            let path_bytes = &entry.dir_path[..path_len];
            let path_str = match core::str::from_utf8(path_bytes) {
                Ok(s) => s,
                Err(_) => return -22,
            };

            // Get directory entries as newline-separated names
            let mut name_buf = [0u8; 2048];
            let n = crate::kernel::vfs::vfs_readdir(path_str, &mut name_buf);
            if n < 0 { return n; }

            // `offset` tracks how many bytes of name_buf we've consumed
            let start_off = entry.offset;
            let names_slice = &name_buf[..n as usize];
            let remaining = if start_off < n as usize { &names_slice[start_off..] } else { return 0; };

            let mut buf_pos = 0usize;
            let mut consumed = 0usize;
            let mut ino: u64 = 1000 + start_off as u64;

            for line in remaining.split(|&b| b == b'\n') {
                if line.is_empty() { consumed += 1; continue; }
                let is_dir = line.last() == Some(&b'/');
                let name = if is_dir { &line[..line.len()-1] } else { line };
                let name_len = name.len();

                // dirent64 size: 8+8+2+1 = 19 + name_len + 1, rounded up to 8
                let base = 19 + name_len + 1;
                let reclen = (base + 7) & !7;

                if buf_pos + reclen > buf.len() { break; }

                // d_ino
                core::ptr::write_unaligned(buf.as_mut_ptr().add(buf_pos) as *mut u64, ino);
                // d_off (next entry offset, use consumed+name_len+1)
                core::ptr::write_unaligned(buf.as_mut_ptr().add(buf_pos + 8) as *mut i64,
                    (start_off + consumed + line.len() + 1) as i64);
                // d_reclen
                core::ptr::write_unaligned(buf.as_mut_ptr().add(buf_pos + 16) as *mut u16,
                    reclen as u16);
                // d_type
                buf[buf_pos + 18] = if is_dir { DT_DIR } else { DT_REG };
                // d_name (null-terminated)
                buf[buf_pos + 19..buf_pos + 19 + name_len].copy_from_slice(name);
                buf[buf_pos + 19 + name_len] = 0;
                // zero padding
                for i in (19 + name_len + 1)..reclen {
                    if buf_pos + i < buf.len() { buf[buf_pos + i] = 0; }
                }

                buf_pos += reclen;
                consumed += line.len() + 1; // +1 for the '\n'
                ino += 1;
            }

            // Update offset so next call continues where we left off
            (*sched).tasks[idx].fd_table.entries[fd as usize]
                .as_mut().unwrap().offset = start_off + consumed;

            buf_pos as i64
        }
    }

    fn readdir_impl(&mut self, path: &[u8], buf: &mut [u8]) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22, // EINVAL
        };
        crate::kernel::vfs::vfs_readdir(path_str, buf)
    }

    fn mkdir_impl(&mut self, path: &[u8]) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22,
        };
        unsafe { crate::kernel::vfs::vfs_mkdir(path_str) }
    }

    fn chdir_impl(&mut self, path: &[u8]) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22,
        };
        unsafe { crate::kernel::vfs::vfs_chdir(path_str) }
    }

    fn getcwd_impl(&mut self, buf: &mut [u8]) -> i64 {
        unsafe {
            let sched = &raw const crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            let task  = &(*sched).tasks[idx];
            let len   = task.cwd_len.min(buf.len());
            buf[..len].copy_from_slice(&task.cwd[..len]);
            len as i64
        }
    }

    fn stat_impl(&mut self, path: &[u8], buf_ptr: u64) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22,
        };
        unsafe {
            crate::kernel::vfs::vfs_stat_linux(
                path_str,
                buf_ptr as *mut crate::kernel::vfs::LinuxStat,
            )
        }
    }

    fn lseek_impl(&mut self, fd: i32, offset: i64, whence: u32) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::fs::ramfs::FdBackend;

            if fd < 0 || fd as usize >= crate::kernel::fs::ramfs::MAX_FD { return -9; }
            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let entry = match (*sched).tasks[idx].fd_table.entries[fd as usize] {
                None    => return -9, // EBADF
                Some(e) => e,
            };
            match entry.backend {
                FdBackend::Fat16 => {
                    // fat::seek(raw_fd, offset, whence) → new position or negative
                    let new_pos = crate::kernel::fat::file_seek(entry.raw_fd, offset, whence);
                    new_pos
                }
                FdBackend::RamFS => {
                    if let Some(fs) = crate::kernel::fs::ramfs::RAMFS.get() {
                        let inode = &fs.inodes[entry.inode_idx];
                        let size = inode.data.len() as i64;
                        let cur  = entry.offset as i64;
                        let new_off = match whence {
                            0 => offset,          // SEEK_SET
                            1 => cur + offset,    // SEEK_CUR
                            2 => size + offset,   // SEEK_END
                            _ => return -22,
                        };
                        if new_off < 0 { return -22; }
                        (*sched).tasks[idx].fd_table.entries[fd as usize]
                            .as_mut().unwrap().offset = new_off as usize;
                        new_off
                    } else { -9 }
                }
                _ => -29, // ESPIPE (pipes, ttys, sockets)
            }
        }
    }

    fn readv_impl(&mut self, fd: i32, iov_ptr: u64, iovcnt: u32) -> i64 {
        let mut total: i64 = 0;
        for i in 0..iovcnt as u64 {
            let iov_entry = iov_ptr + i * 16;
            let base = unsafe { core::ptr::read_unaligned(iov_entry as *const u64) };
            let len  = unsafe { core::ptr::read_unaligned((iov_entry + 8) as *const u64) };
            if len == 0 { continue; }
            let buf = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, len as usize) };
            let r = self.fs_read(fd, buf);
            if r < 0 { return if total > 0 { total } else { r }; }
            total += r;
            if r < len as i64 { break; }
        }
        total
    }

    fn writev_impl(&mut self, fd: i32, iov_ptr: u64, iovcnt: u32) -> i64 {
        let mut total: i64 = 0;
        for i in 0..iovcnt as u64 {
            let iov_entry = iov_ptr + i * 16;
            let base = unsafe { core::ptr::read_unaligned(iov_entry as *const u64) };
            let len  = unsafe { core::ptr::read_unaligned((iov_entry + 8) as *const u64) };
            if len == 0 { continue; }
            let buf = unsafe { core::slice::from_raw_parts(base as *const u8, len as usize) };
            let r = if fd == 1 || fd == 2 {
                self.write_console(buf);
                len as i64
            } else {
                self.fs_write_file(fd, buf)
            };
            if r < 0 { return if total > 0 { total } else { r }; }
            total += r;
        }
        total
    }

    fn access_impl(&mut self, path: &[u8], _mode: u32) -> i64 {
        // Just check existence via stat
        let path_str = match core::str::from_utf8(path) {
            Ok(s) => s,
            Err(_) => return -22,
        };
        let mut dummy = crate::kernel::vfs::LinuxStat::zeroed();
        unsafe { crate::kernel::vfs::vfs_stat_linux(path_str, &mut dummy) }
    }

    fn dup_impl(&mut self, fd: i32) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::fs::ramfs::MAX_FD;
            if fd < 0 || fd as usize >= MAX_FD { return -9; }
            let sched = &raw mut SCHED;
            let idx = CURRENT_TASK_IDX;
            let entry = match (*sched).tasks[idx].fd_table.entries[fd as usize] {
                None => return -9,
                Some(e) => e,
            };
            // Find lowest unused fd ≥ 0
            for new_fd in 0..MAX_FD {
                if (*sched).tasks[idx].fd_table.entries[new_fd].is_none() {
                    (*sched).tasks[idx].fd_table.entries[new_fd] = Some(entry);
                    return new_fd as i64;
                }
            }
            -24 // EMFILE
        }
    }

    fn ftruncate_impl(&mut self, fd: i32, length: u64) -> i64 {
        // Alias to truncate_impl using fd — only FAT16 supported for now
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::fs::ramfs::FdBackend;
            if fd < 0 || fd as usize >= crate::kernel::fs::ramfs::MAX_FD { return -9; }
            let sched = &raw const SCHED;
            let idx = CURRENT_TASK_IDX;
            let entry = match (*sched).tasks[idx].fd_table.entries[fd as usize] {
                None => return -9,
                Some(e) => e,
            };
            match entry.backend {
                FdBackend::Fat16 => 0, // FAT16 truncate not yet implemented
                _ => 0,
            }
        }
    }

    fn rmdir_impl(&mut self, path: &[u8]) -> i64 {
        // FAT16 rmdir — for now alias to unlink (FAT16 treats empty dirs like files for removal)
        self.unlink_impl(path)
    }

    fn fchdir_impl(&mut self, fd: i32) -> i64 {
        // Stub: we don't track per-fd path info; return success
        let _ = fd;
        0
    }

    fn getrlimit_impl(&mut self, resource: u32, rlim_ptr: u64) -> i64 {
        // struct rlimit { rlim_cur: u64, rlim_max: u64 } = 16 bytes
        // Return RLIM_INFINITY for all resources
        if rlim_ptr == 0 { return -22; }
        const RLIM_INF: u64 = u64::MAX;
        unsafe {
            core::ptr::write_unaligned(rlim_ptr as *mut u64, RLIM_INF);
            core::ptr::write_unaligned((rlim_ptr + 8) as *mut u64, RLIM_INF);
        }
        let _ = resource;
        0
    }

    fn getrusage_impl(&mut self, _who: i32, buf_ptr: u64) -> i64 {
        // struct rusage is 144 bytes — zero it out
        if buf_ptr == 0 { return -22; }
        unsafe {
            core::ptr::write_bytes(buf_ptr as *mut u8, 0, 144);
        }
        0
    }

    fn sysinfo_impl(&mut self, buf_ptr: u64) -> i64 {
        // struct sysinfo (Linux) = 112 bytes
        if buf_ptr == 0 { return -22; }
        unsafe {
            core::ptr::write_bytes(buf_ptr as *mut u8, 0, 112);
            // uptime at offset 0 (i64), totalram at 8 (u64)
            let ticks = crate::kernel::timer::get_ticks();
            core::ptr::write_unaligned(buf_ptr as *mut i64, (ticks / 100) as i64);
            core::ptr::write_unaligned((buf_ptr + 8) as *mut u64, 128 * 1024 * 1024); // 128MB total
            core::ptr::write_unaligned((buf_ptr + 16) as *mut u64, 64 * 1024 * 1024); // 64MB free
            core::ptr::write_unaligned((buf_ptr + 104) as *mut u32, 4096); // mem_unit
        }
        0
    }

    fn pread64_impl(&mut self, fd: i32, buf_ptr: u64, count: u64, offset: i64) -> i64 {
        // Save position, seek, read, restore
        let saved = self.lseek_impl(fd, 0, 1); // SEEK_CUR
        if saved < 0 { return saved; }
        let r = self.lseek_impl(fd, offset, 0); // SEEK_SET
        if r < 0 { return r; }
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, count as usize) };
        let n = self.fs_read(fd, buf);
        let _ = self.lseek_impl(fd, saved, 0); // restore
        n
    }

    fn pwrite64_impl(&mut self, fd: i32, buf_ptr: u64, count: u64, offset: i64) -> i64 {
        let saved = self.lseek_impl(fd, 0, 1);
        if saved < 0 { return saved; }
        let r = self.lseek_impl(fd, offset, 0);
        if r < 0 { return r; }
        let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, count as usize) };
        let n = self.fs_write_file(fd, buf);
        let _ = self.lseek_impl(fd, saved, 0);
        n
    }

    fn fstat_impl(&mut self, fd: i32, buf_ptr: u64) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::fs::ramfs::FdBackend;
            use crate::kernel::vfs::{LinuxStat, S_IFIFO};

            if fd < 0 || fd as usize >= crate::kernel::fs::ramfs::MAX_FD { return -9; }
            let sched = &raw const SCHED;
            let idx   = CURRENT_TASK_IDX;
            let entry = match (*sched).tasks[idx].fd_table.entries[fd as usize] {
                None    => return -9,
                Some(e) => e,
            };
            let out = buf_ptr as *mut LinuxStat;
            *out = LinuxStat::zeroed();

            match entry.backend {
                FdBackend::DevNull | FdBackend::DevTty => {
                    *out = LinuxStat::fill_chardev(fd as u64 + 1);
                    0
                }
                FdBackend::Fat16 => {
                    let size = crate::kernel::fat::file_size(entry.raw_fd) as u64;
                    *out = LinuxStat::fill_file(size, 100 + entry.raw_fd as u64);
                    0
                }
                FdBackend::RamFS => {
                    let size = match crate::kernel::fs::ramfs::RAMFS.get() {
                        Some(fs) => fs.inodes[entry.inode_idx].data.len() as u64,
                        None => 0,
                    };
                    *out = LinuxStat::fill_file(size, 300 + entry.inode_idx as u64);
                    0
                }
                FdBackend::Pipe => {
                    (*out).st_mode = S_IFIFO | 0o666;
                    (*out).st_ino  = 500 + fd as u64;
                    0
                }
                FdBackend::Ext2 => {
                    *out = LinuxStat::fill_file(0, 200 + entry.raw_fd as u64);
                    0
                }
                FdBackend::Dir => {
                    *out = LinuxStat::fill_dir(600 + fd as u64);
                    0
                }
            }
        }
    }

    fn poll_impl(&mut self, fds_ptr: u64, nfds: u64, timeout_ms: i64) -> i64 {
        use crate::kernel::fs::ramfs::{FdBackend, MAX_FD};
        use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
        use super::syscall_core::TIMER_HZ;

        #[repr(C)]
        struct PollFd { fd: i32, events: i16, revents: i16 }
        const POLLIN:  i16 = 0x0001;
        const POLLOUT: i16 = 0x0004;
        const POLLHUP: i16 = 0x0010;

        let n = nfds as usize;
        if n == 0 { return 0; }
        if n > 64 || fds_ptr < 0x1000 { return -22; } // EINVAL / EFAULT

        let fds = unsafe {
            core::slice::from_raw_parts_mut(fds_ptr as *mut PollFd, n)
        };

        let mut ready = 0i64;
        unsafe {
            let sched = &raw const SCHED;
            let task  = &(*sched).tasks[CURRENT_TASK_IDX];

            for pfd in fds.iter_mut() {
                pfd.revents = 0;
                let fd = pfd.fd;
                if fd < 0 { continue; }

                // Socket FDs (fd >= 200)
                if fd >= 200 {
                    if (pfd.events & POLLIN) != 0
                        && crate::kernel::net::socket::socket_read_ready(fd as i64)
                    {
                        pfd.revents |= POLLIN;
                    }
                    if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                    if pfd.revents != 0 { ready += 1; }
                    continue;
                }

                if fd as usize >= MAX_FD { continue; }

                let entry = match task.fd_table.entries[fd as usize] {
                    None    => { pfd.revents = POLLHUP; ready += 1; continue; }
                    Some(e) => e,
                };

                match entry.backend {
                    FdBackend::DevTty => {
                        if (pfd.events & POLLIN) != 0
                            && crate::kernel::stdin::available() > 0
                        {
                            pfd.revents |= POLLIN;
                        }
                        if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                    }
                    FdBackend::DevNull => {
                        if (pfd.events & POLLIN)  != 0 { pfd.revents |= POLLIN; }
                        if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                    }
                    FdBackend::Pipe => {
                        if entry.writable {
                            if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                        } else {
                            if (pfd.events & POLLIN) != 0
                                && crate::kernel::pipe::read_ready(entry.raw_fd)
                            {
                                pfd.revents |= POLLIN;
                            }
                        }
                    }
                    FdBackend::RamFS | FdBackend::Fat16 | FdBackend::Ext2 | FdBackend::Dir => {
                        if (pfd.events & POLLIN)  != 0 { pfd.revents |= POLLIN; }
                        if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                    }
                }
                if pfd.revents != 0 { ready += 1; }
            }
        }

        if ready > 0 || timeout_ms == 0 {
            return ready;
        }

        // Nothing ready — sleep then return 0 (user retries poll).
        let ticks = if timeout_ms < 0 {
            2  // ~20 ms at 100 Hz: yield so other tasks can produce data
        } else {
            ((timeout_ms as u64 * TIMER_HZ) / 1000).max(1)
        };
        self.sleep_until_tick(self.current_ticks() + ticks);
        0  // reached only in busy-wait fallback
    }

    fn dup2_impl(&mut self, old_fd: i32, new_fd: i32) -> i64 {
        unsafe {
            let sched = &raw mut crate::kernel::scheduler::SCHED;
            let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
            (*sched).tasks[idx].fd_table.dup2(old_fd, new_fd)
        }
    }

    fn ioctl_impl(&mut self, fd: i32, request: u64, arg: u64) -> i64 {
        unsafe { crate::kernel::tty::ioctl(fd, request, arg) }
    }

    fn chmod_impl(&mut self, path: &[u8], mode: u16) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22,
        };
        match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
            None     => -2,
            Some(fs) => {
                match fs.resolve(path_str) {
                    None      => -7, // ENOENT
                    Some(idx) => {
                        let fs_mut = unsafe {
                            &mut *(fs as *const _ as *mut crate::kernel::fs::ramfs::RamFs)
                        };
                        fs_mut.inodes[idx].mode = mode;
                        0
                    }
                }
            }
        }
    }

    fn chown_impl(&mut self, path: &[u8], uid: u32, gid: u32) -> i64 {
        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -22,
        };
        match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
            None     => -2,
            Some(fs) => {
                match fs.resolve(path_str) {
                    None      => -7,
                    Some(idx) => {
                        let fs_mut = unsafe {
                            &mut *(fs as *const _ as *mut crate::kernel::fs::ramfs::RamFs)
                        };
                        fs_mut.inodes[idx].uid = uid;
                        fs_mut.inodes[idx].gid = gid;
                        0
                    }
                }
            }
        }
    }

    fn unlink_impl(&mut self, path: &[u8]) -> i64 {
        let path_str = match core::str::from_utf8(path) { Ok(s) => s, Err(_) => return -22 };
        match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
            None => -2,
            Some(fs) => {
                match fs.remove_file(path_str) {
                    Ok(removed_idx) => {
                        // Fix up open FD tables in all tasks.
                        unsafe {
                            use crate::kernel::scheduler::SCHED;
                            for task in (*core::ptr::addr_of_mut!(SCHED)).tasks.iter_mut() {
                                task.fd_table.on_inode_removed(removed_idx);
                            }
                        }
                        0
                    }
                    Err(e) => e,
                }
            }
        }
    }

    fn rename_impl(&mut self, old_path: &[u8], new_path: &[u8]) -> i64 {
        let old = match core::str::from_utf8(old_path) { Ok(s) => s, Err(_) => return -22 };
        let new = match core::str::from_utf8(new_path) { Ok(s) => s, Err(_) => return -22 };
        match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
            None => -2,
            Some(fs) => match fs.rename(old, new) { Ok(()) => 0, Err(e) => e },
        }
    }

    fn truncate_impl(&mut self, fd: i32, length: u64) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            use crate::kernel::fs::ramfs::{FdBackend, RAMFS};

            if fd < 0 || fd as usize >= crate::kernel::fs::ramfs::MAX_FD { return -9; }
            let sched = &raw const SCHED;
            let idx   = CURRENT_TASK_IDX;
            let entry = match (*sched).tasks[idx].fd_table.entries[fd as usize] {
                None    => return -9,
                Some(e) => e,
            };
            match entry.backend {
                FdBackend::RamFS => {
                    if let Some(fs) = RAMFS.get() {
                        fs.truncate_by_idx(entry.inode_idx, length as usize);
                        0
                    } else { -2 }
                }
                _ => -38, // ENOTSOCK — not a regular file
            }
        }
    }

    fn getenv_impl(&mut self, key: &[u8], buf: &mut [u8]) -> i64 {
        match crate::kernel::env::getenv(key) {
            None => -7, // ENOENT
            Some(val) => {
                let n = val.len().min(buf.len());
                buf[..n].copy_from_slice(&val[..n]);
                n as i64
            }
        }
    }

    fn setenv_impl(&mut self, key: &[u8], val: &[u8]) -> i64 {
        if crate::kernel::env::setenv(key, val) { 0 } else { -1 }
    }

    fn shmget_impl(&mut self, key: u32, size: u64, flags: u32) -> i64 {
        unsafe { crate::kernel::shm::shmget(key, size, flags) }
    }

    fn shmat_impl(&mut self, shmid: u32, _addr_hint: u64) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let cr3   = (*sched).tasks[idx].cr3;
            let att   = &raw mut (*sched).tasks[idx].shm_attaches;
            crate::kernel::shm::shmat(shmid, &mut *att, cr3)
        }
    }

    fn shmdt_impl(&mut self, addr: u64) -> i64 {
        unsafe {
            use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let att   = &raw mut (*sched).tasks[idx].shm_attaches;
            crate::kernel::shm::shmdt(addr, &mut *att)
        }
    }

    fn sigaction_impl(&mut self, signum: u32, handler: u64, old_ptr: u64) -> i64 {
        use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, NSIG};
        if signum == 0 || signum as usize >= NSIG { return -22; } // EINVAL
        unsafe {
            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let prev  = (*sched).tasks[idx].signal_handlers[signum as usize];
            if old_ptr != 0 {
                if let Err(_) = crate::kernel::syscall_core::validate_user_range(old_ptr, 8) {
                    return -14; // EFAULT
                }
                core::ptr::write_unaligned(old_ptr as *mut u64, prev);
            }
            (*sched).tasks[idx].signal_handlers[signum as usize] = handler;
        }
        0
    }

    fn sigreturn_impl(&mut self) -> i64 {
        use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, SignalFrame};
        use crate::kernel::paging_allocator;
        unsafe {
            let sched = &raw mut SCHED;
            let idx   = CURRENT_TASK_IDX;
            let task  = &raw mut (*sched).tasks[idx];
            let cr3   = (*task).cr3;

            // RSP currently points at the saved SignalFrame.
            let rsp = (*task).ctx.rsp;
            let frame_size = core::mem::size_of::<SignalFrame>() as u64;

            // Read the frame from user memory via a temporary CR3 switch.
            let mut frame = SignalFrame {
                rip: 0, rax: 0, rbx: 0, rcx: 0, rdx: 0,
                rsi: 0, rdi: 0, r8:  0, r9:  0, r10: 0,
                r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
                rbp: 0, rflags: 0,
            };
            {
                let saved_cr3: u64;
                core::arch::asm!("mov {}, cr3", out(reg) saved_cr3, options(nostack, nomem));
                core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, nomem));
                core::ptr::copy_nonoverlapping(
                    rsp as *const u8,
                    &raw mut frame as *mut u8,
                    frame_size as usize,
                );
                core::arch::asm!("mov cr3, {}", in(reg) saved_cr3, options(nostack, nomem));
            }

            // Restore all general-purpose registers and RIP.
            (*task).ctx.rip    = frame.rip;
            (*task).ctx.rax    = frame.rax;
            (*task).ctx.rbx    = frame.rbx;
            (*task).ctx.rcx    = frame.rcx;
            (*task).ctx.rdx    = frame.rdx;
            (*task).ctx.rsi    = frame.rsi;
            (*task).ctx.rdi    = frame.rdi;
            (*task).ctx.r8     = frame.r8;
            (*task).ctx.r9     = frame.r9;
            (*task).ctx.r10    = frame.r10;
            (*task).ctx.r11    = frame.r11;
            (*task).ctx.r12    = frame.r12;
            (*task).ctx.r13    = frame.r13;
            (*task).ctx.r14    = frame.r14;
            (*task).ctx.r15    = frame.r15;
            (*task).ctx.rbp    = frame.rbp;
            (*task).ctx.rflags = frame.rflags;
            // Pop frame + return address (8 bytes) off the stack.
            (*task).ctx.rsp    = rsp + frame_size + 8;
        }
        0
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

    // ── GUI process syscalls ───────────────────────────────────────────────

    unsafe fn gui_create_impl(&mut self, pid: u64, title: &[u8], w: u32, h: u32) -> i64 {
        unsafe { crate::kernel::gui_proc::create_window(pid as u32, title, w, h) }
    }

    fn gui_destroy_impl(&mut self, pid: u64, win_id: u32) -> i64 {
        unsafe { crate::kernel::gui_proc::destroy_window(pid as u32, win_id) }
    }

    fn gui_fill_rect_impl(&mut self, pid: u64, win_id: u32,
                          x: u32, y: u32, w: u32, h: u32, color: u32) -> i64 {
        unsafe { crate::kernel::gui_proc::fill_rect(pid as u32, win_id, x, y, w, h, color) }
    }

    unsafe fn gui_draw_text_impl(&mut self, pid: u64, win_id: u32,
                                 x: u32, y: u32, color: u32, text: &[u8]) -> i64 {
        unsafe { crate::kernel::gui_proc::draw_text(pid as u32, win_id, x, y, color, text) }
    }

    fn gui_present_impl(&mut self, pid: u64, win_id: u32) -> i64 {
        unsafe { crate::kernel::gui_proc::present(pid as u32, win_id) }
    }

    fn gui_poll_event_impl(&mut self, pid: u64, win_id: u32, event_ptr: u64) -> i64 {
        unsafe { crate::kernel::gui_proc::poll_event(pid as u32, win_id, event_ptr) }
    }

    fn gui_get_size_impl(&mut self, pid: u64, win_id: u32, w_ptr: u64, h_ptr: u64) -> i64 {
        unsafe { crate::kernel::gui_proc::get_size(pid as u32, win_id, w_ptr, h_ptr) }
    }

    fn gui_blit_shm_impl(&mut self, pid: u64, win_id: u32, shm_id: u32,
                         sx: u32, sy: u32, sw: u32, sh: u32, dx: u32, dy: u32) -> i64 {
        unsafe { crate::kernel::gui_proc::blit_shm(pid as u32, win_id, shm_id,
                                                    sx, sy, sw, sh, dx, dy) }
    }

    fn install_query_impl(&mut self) -> i64 {
        if !crate::kernel::ata::is_present_sec() { return -1; }
        crate::kernel::ata::sector_count_sec() as i64
    }

    fn install_begin_impl(&mut self) -> i64 {
        unsafe { crate::kernel::installer::do_install() }
    }
}

// ── exec helpers (not part of the trait; called via exec_program) ─────────

impl KernelRuntime {
    /// Resolve path → binary bytes, then hand off to `exec_binary`.
    /// `extra_args` is the space-separated argument string (argv[1..]).
    fn exec_resolve(&mut self, path: &[u8], extra_args: &str) -> i64 {
        extern crate alloc;
        use alloc::vec::Vec;

        let path_str = match core::str::from_utf8(path) {
            Ok(s)  => s,
            Err(_) => return -1,
        };

        // Derive argv[0] from the path (basename without leading slashes).
        let prog_name = path_str
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path_str);

        // 1. Built-in registry (embedded binaries — no disk needed).
        let short = path_str.trim_start_matches('/').trim_start_matches("bin/");
        if let Some(b) = crate::kernel::programs::find(short) {
            return self.exec_binary(b, prog_name, extra_args);
        }

        // 2. RamFS
        if let Some(data) = unsafe { crate::kernel::fs::ramfs::RAMFS.get() }
            .and_then(|fs| fs.read_file(path_str))
        {
            if !data.is_empty() {
                let owned: Vec<u8> = data.to_vec();
                return self.exec_binary(&owned, prog_name, extra_args);
            }
        }

        // 3. FAT16
        if path.starts_with(b"/disk/") {
            return self.exec_fat(path, extra_args);
        }

        -2 // ENOENT
    }

    fn exec_fat(&mut self, path: &[u8], extra_args: &str) -> i64 {
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
        let path_str = core::str::from_utf8(path).unwrap_or("");
        let prog_name = path_str.trim_end_matches('/').rsplit('/').next().unwrap_or(path_str);
        self.exec_binary(&buf, prog_name, extra_args)
    }

    /// Load `binary` into a fresh address space and replace the current task.
    /// `prog_name` is the argv[0] string; `extra_args` is the space-separated
    /// argument string (argv[1..]).  On success this never returns.
    fn exec_binary(&mut self, binary: &[u8], prog_name: &str, extra_args: &str) -> i64 {
        extern crate alloc;
        use alloc::vec::Vec;
        use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, EXIT_PREEMPTED,
                                       USER_SIGTRAMP, SIGTRAMP_BYTES, write_argv_to_stack};
        use crate::kernel::paging_allocator as pa;
        use crate::kernel::fs::ramfs::FdTable;

        const PAGE_SIZE:        usize = 4096;
        const USER_STACK_TOP:   u64   = 0x0080_0000;
        const USER_STACK_PAGES: usize = 64; // 256 KB
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

        // Map signal trampoline.
        if unsafe { pa::map_user_region_in(new_cr3, USER_SIGTRAMP, 1, true, true) }.is_ok() {
            unsafe { pa::copy_to_region_in(new_cr3, USER_SIGTRAMP, SIGTRAMP_BYTES); }
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

        // Build argv: argv[0] = prog_name, then split extra_args by whitespace.
        let mut argv_buf: Vec<&str> = Vec::new();
        argv_buf.push(prog_name);
        for token in extra_args.split_ascii_whitespace() {
            if argv_buf.len() >= 31 { break; }
            argv_buf.push(token);
        }
        let initial_rsp = unsafe { write_argv_to_stack(new_cr3, USER_STACK_TOP, &argv_buf) };

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
            (*task).cr3         = new_cr3;
            (*task).entry       = entry;
            (*task).first_run   = true;
            (*task).initial_rsp = initial_rsp;
            (*task).fd_table    = FdTable::new();
            (*task).fd_table.entries[0] = saved_std[0];
            (*task).fd_table.entries[1] = saved_std[1];
            (*task).fd_table.entries[2] = saved_std[2];
            (*task).output_len  = 0;
        }

        // Free old page table (user half only; kernel half is shared).
        if old_cr3 != 0 {
            unsafe { pa::free_user_page_table(old_cr3); }
        }

        // Non-local goto back to tick().  tick() will see EXIT_PREEMPTED,
        // mark the task Ready, and on the next tick launch_at(entry, initial_rsp, new_cr3).
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
