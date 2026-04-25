//! Multi-process preemptive round-robin scheduler for OxideOS.
//!
//! Supports up to `MAX_TASKS` concurrent user-mode tasks, each with its own
//! CR3 (per-process page table), captured stdout buffer, and saved register
//! context.  The timer ISR preempts the running task; `tick()` selects the
//! next ready task in round-robin order.
//!
//! # Address layout (per task)
//! Code: `0x0040_0000`  Stack top: `0x0080_0000`
//! All tasks share the same virtual addresses — per-process CR3 maps them to
//! different physical frames.

use crate::kernel::paging_allocator;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::user_mode::TaskContext;
use crate::kernel::fs::ramfs::FdTable;

pub const MAX_TASKS:       usize = 8;
const  PAGE_SIZE:          usize = 4096;
const  USER_CODE_ADDR:     u64   = 0x0040_0000;
const  USER_STACK_TOP:     u64   = 0x0080_0000;
const  USER_STACK_PAGES:   usize = 64; // 256 KB — Rust programs need deep stacks
const  TASK_OUTPUT_CAP:    usize = 2048;

/// Timer ticks a task runs before being preempted (100 Hz → 20 ms).
pub const TICKS_PER_SLICE: u64 = 2;

/// Sentinel: timer ISR preempted the task.
pub const EXIT_PREEMPTED: i64 = i64::MIN;
/// Sentinel: task called the Sleep syscall.
pub const EXIT_SLEEPING:  i64 = i64::MIN + 1;

// ── Task state ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TaskState {
    Empty,
    Ready,
    Running,
    Sleeping(u64),           // wake at this tick
    Waiting(u8),             // waiting for child with this PID to die
    WaitingForMsg(u32, u64), // blocking msgrcv: (queue_id, user msg_out ptr)
    Dead(i64),               // exit code (pages already freed)
}

impl TaskState {
    pub fn exit_code(self) -> Option<i64> {
        if let TaskState::Dead(code) = self { Some(code) } else { None }
    }
}

pub const CWD_MAX: usize = 128;

/// Maximum number of tracked anonymous mmap regions per process.
pub const MAX_MMAP_REGIONS: usize = 32;

/// A tracked anonymous mmap allocation (virtual_base, page_count).
#[derive(Clone, Copy)]
pub struct MmapRegion {
    pub virt:  u64,
    pub pages: u32,
    pub _pad:  u32,
}
impl MmapRegion {
    pub const fn empty() -> Self { Self { virt: 0, pages: 0, _pad: 0 } }
}

/// Number of signal slots (POSIX requires at least 32).
pub const NSIG: usize = 32;

/// SIG_DFL — use the default action for this signal (0).
pub const SIG_DFL: u64 = 0;
/// SIG_IGN — ignore this signal (1).
pub const SIG_IGN: u64 = 1;

// Common signal numbers (POSIX).
pub const SIGHUP:  u8 = 1;
pub const SIGINT:  u8 = 2;
pub const SIGQUIT: u8 = 3;
pub const SIGALRM: u8 = 14;
pub const SIGKILL: u8 = 9;
pub const SIGTERM: u8 = 15;
pub const SIGCHLD: u8 = 17;
pub const SIGCONT: u8 = 18;
pub const SIGSTOP: u8 = 19;
pub const SIGTSTP: u8 = 20;

// sigprocmask "how" values (Linux ABI)
pub const SIG_BLOCK:   u32 = 0;
pub const SIG_UNBLOCK: u32 = 1;
pub const SIG_SETMASK: u32 = 2;

/// Virtual address of the signal-return trampoline page mapped into every process.
/// Sits between the stack (tops at 0x0080_0000) and the heap base (0x0100_0000).
pub const USER_SIGTRAMP: u64 = 0x0090_0000;

/// Trampoline machine code: `mov rax, 95; int 0x80; ud2`
/// When a signal handler returns, it `ret`s here, which calls SIGRETURN.
pub const SIGTRAMP_BYTES: &[u8] = &[
    0x48, 0xc7, 0xc0, 95, 0, 0, 0,  // mov rax, 95
    0xcd, 0x80,                       // int 0x80
    0x0f, 0x0b,                       // ud2 (should not reach here)
];

/// Signal frame saved on the user stack during signal delivery.
/// The kernel pushes this before jumping to the handler; SIGRETURN pops it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SignalFrame {
    pub rip:    u64,
    pub rax:    u64,
    pub rbx:    u64,
    pub rcx:    u64,
    pub rdx:    u64,
    pub rsi:    u64,
    pub rdi:    u64,
    pub r8:     u64,
    pub r9:     u64,
    pub r10:    u64,
    pub r11:    u64,
    pub r12:    u64,
    pub r13:    u64,
    pub r14:    u64,
    pub r15:    u64,
    pub rbp:    u64,
    pub rflags: u64,
}

pub struct Task {
    pub state:      TaskState,
    pub ctx:        TaskContext,
    pub name:       [u8; 16],
    pub name_len:   usize,
    pub first_run:  bool,
    pub entry:      u64,
    pub cr3:        u64,
    pub pid:        u8,
    /// PID of the parent that fork'd this task; 0 = no parent.
    pub parent_pid: u8,
    /// Process group ID. 0 means "same as pid" (set on first use).
    pub pgid:       u8,
    /// IA32_FS_BASE MSR value for this task's TLS pointer (set via arch_prctl).
    pub fs_base:    u64,
    /// Current userspace heap break (virtual address).  0 = unset (use USER_HEAP_BASE).
    pub heap_end:   u64,
    /// Top of the anonymous-mmap area.  0 = unset (use USER_MMAP_BASE).
    pub mmap_end:   u64,
    pub output:     [u8; TASK_OUTPUT_CAP],
    pub output_len: usize,
    /// Per-process open file-descriptor table.
    /// FDs 0/1/2 (stdin/stdout/stderr) are reserved; real files start at FD 3.
    pub fd_table:   FdTable,
    /// Current working directory (null-terminated UTF-8 path).
    pub cwd:        [u8; CWD_MAX],
    pub cwd_len:    usize,
    /// Bitmask of pending signals (bit N = signal N+1 is pending).
    pub pending_signals: u32,
    /// Bitmask of blocked signals (sigprocmask). SIGKILL/SIGSTOP can't be blocked.
    pub signal_mask: u32,
    /// Saved mask to restore after sigsuspend's signal handler returns.
    pub saved_signal_mask: u32,
    /// Set while a sigsuspend is in progress; cleared by sigreturn.
    pub in_sigsuspend: bool,
    /// Tick at which SIGALRM fires; 0 = no alarm armed.
    pub alarm_deadline: u64,
    /// Per-signal handler addresses (index = signal number).
    /// 0 (SIG_DFL) = default action; 1 (SIG_IGN) = ignore.
    pub signal_handlers: [u64; NSIG],
    /// Shared memory attachments for this process.
    pub shm_attaches: [crate::kernel::shm::ShmAttach; crate::kernel::shm::MAX_ATTACH],
    /// Initial RSP for the first-run launch — points to the argc value on the
    /// user stack (System V AMD64 ABI). Set by spawn() / exec_binary().
    pub initial_rsp: u64,
    /// Tracked anonymous mmap allocations (for munmap).
    pub mmap_regions:  [MmapRegion; MAX_MMAP_REGIONS],
    pub mmap_nregions: usize,
}

impl Task {
    const fn empty() -> Self {
        let mut cwd = [0u8; CWD_MAX];
        cwd[0] = b'/';
        Self {
            state:      TaskState::Empty,
            ctx:        TaskContext::zeroed(),
            name:       [0u8; 16],
            name_len:   0,
            first_run:  true,
            entry:      USER_CODE_ADDR,
            cr3:        0,
            pid:        0,
            parent_pid: 0,
            pgid:       0,
            fs_base:    0,
            heap_end:   0,
            mmap_end:   0,
            output:     [0u8; TASK_OUTPUT_CAP],
            output_len: 0,
            fd_table:   FdTable::new(),
            cwd,
            cwd_len:    1, // "/"
            pending_signals: 0,
            signal_mask: 0,
            saved_signal_mask: 0,
            in_sigsuspend: false,
            alarm_deadline: 0,
            signal_handlers: [0u64; NSIG],
            shm_attaches: [const { crate::kernel::shm::ShmAttach::empty() }; crate::kernel::shm::MAX_ATTACH],
            initial_rsp: USER_STACK_TOP - 16,
            mmap_regions:  [const { MmapRegion::empty() }; MAX_MMAP_REGIONS],
            mmap_nregions: 0,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }
}

// ── Scheduler state ────────────────────────────────────────────────────────

pub struct Scheduler {
    pub tasks:           [Task; MAX_TASKS],
    pub current:         usize,
    pub slice_remaining: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            tasks:           [const { Task::empty() }; MAX_TASKS],
            current:         0,
            slice_remaining: 0,
        }
    }
}

pub static mut SCHED: Scheduler = Scheduler::new();

/// Index of the task currently executing (set before every launch/resume).
/// Read by the timer ISR and by `output_write_for_task`.
pub static mut CURRENT_TASK_IDX: usize = 0;

// ── Per-task output capture ────────────────────────────────────────────────

/// Append bytes to the running task's output buffer.
/// Called from `user_mode::output_write` on every Write(fd=1) syscall.
pub fn output_write_for_task(idx: usize, bytes: &[u8]) {
    if idx >= MAX_TASKS { return; }
    unsafe {
        let used  = SCHED.tasks[idx].output_len;
        let space = TASK_OUTPUT_CAP - used;
        let n     = bytes.len().min(space);
        SCHED.tasks[idx].output[used..used + n].copy_from_slice(&bytes[..n]);
        SCHED.tasks[idx].output_len = used + n;
    }
}

/// Drain task `idx`'s output buffer, calling `f` for each `\n`-terminated
/// line.  Clears the buffer afterwards.
pub fn output_drain_task(idx: usize, mut f: impl FnMut(&str)) {
    if idx >= MAX_TASKS { return; }
    unsafe {
        let len = SCHED.tasks[idx].output_len;
        if len == 0 { return; }
        // Copy to stack buffer to avoid holding a reference into the static.
        let mut tmp = [0u8; TASK_OUTPUT_CAP];
        tmp[..len].copy_from_slice(&SCHED.tasks[idx].output[..len]);
        SCHED.tasks[idx].output_len = 0;
        let data = core::str::from_utf8(&tmp[..len]).unwrap_or("");
        for line in data.split('\n') {
            if !line.is_empty() { f(line); }
        }
    }
}

// ── argv/argc helpers ──────────────────────────────────────────────────────

/// Build a System V AMD64 ABI argv+envp block at the top of the user stack
/// and copy it into the page table at `cr3`.
///
/// Stack layout (little-endian u64 values):
///   [rsp +  0]                        = argc
///   [rsp +  8 .. 8+argc*8]            = argv[0..argc] ptrs
///   [rsp +  8+argc*8]                 = NULL  (end of argv)
///   [rsp +  8+(argc+1)*8 .. ...]      = envp[0..envc] ptrs
///   [rsp +  8+(argc+1+envc)*8]        = NULL  (end of envp)
///   [rsp + ptr_section_size]          = argv strings, then envp "K=V\0" strings
///
/// `stack_top` is the first byte **above** the mapped stack region.
pub unsafe fn write_argv_to_stack(cr3: u64, stack_top: u64, args: &[&str]) -> u64 {
    let argc = args.len().min(31);

    // Collect all env vars as "KEY=VALUE\0" strings into a temporary buffer.
    let mut env_raw = [0u8; 1024];
    let (envc, env_raw_len) = crate::kernel::env::write_env_strings(&mut env_raw);

    // Pointer table: [argc] + [argv*argc] + [NULL] + [envp*envc] + [NULL]
    let ptr_table_bytes = 8 * (1 + argc + 1 + envc + 1);

    let mut argv_str_bytes = 0usize;
    for a in &args[..argc] { argv_str_bytes += a.len() + 1; }

    // Total = pointer table + argv strings + envp strings, aligned.
    let raw_total = ptr_table_bytes + argv_str_bytes + env_raw_len;
    let total = ((raw_total + 15) & !15) + 8;

    if total > 2048 { return stack_top - 8; }

    let initial_rsp = stack_top - total as u64;
    let mut buf = [0u8; 2048];

    // argc
    buf[0..8].copy_from_slice(&(argc as u64).to_le_bytes());

    // argv pointers + argv strings
    let mut str_off = ptr_table_bytes;
    for i in 0..argc {
        let va = initial_rsp + str_off as u64;
        let slot = 8 + i * 8;
        buf[slot..slot + 8].copy_from_slice(&va.to_le_bytes());
        let b = args[i].as_bytes();
        buf[str_off..str_off + b.len()].copy_from_slice(b);
        buf[str_off + b.len()] = 0;
        str_off += b.len() + 1;
    }
    // argv[argc] = NULL: slot = 8 + argc*8, already 0

    // Copy raw env strings after argv strings
    buf[str_off..str_off + env_raw_len].copy_from_slice(&env_raw[..env_raw_len]);

    // envp pointers: scan env_raw to find each "KEY=VAL\0" entry
    let envp_str_base_off = str_off; // buf offset where env strings begin
    let mut scan = 0usize;
    let mut ei   = 0usize;
    while scan < env_raw_len && ei < envc {
        // VA of the start of this env string
        let va = initial_rsp + (envp_str_base_off + scan) as u64;
        let slot = 8 + (argc + 1 + ei) * 8;
        buf[slot..slot + 8].copy_from_slice(&va.to_le_bytes());
        // Advance past this NUL-terminated string
        while scan < env_raw_len && env_raw[scan] != 0 { scan += 1; }
        scan += 1; // skip NUL
        ei   += 1;
    }
    // envp[envc] = NULL: slot = 8 + (argc+1+envc)*8, already 0

    unsafe { paging_allocator::copy_to_region_in(cr3, initial_rsp, &buf[..total]); }
    initial_rsp
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Snapshot used by the `ps` terminal command.
#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub pid:      u8,
    pub name:     [u8; 16],
    pub name_len: usize,
    pub state:    TaskState,
}

pub fn task_infos() -> [TaskInfo; MAX_TASKS] {
    unsafe {
        let sched = &raw const SCHED;
        let mut out = [TaskInfo { pid: 0, name: [0u8; 16], name_len: 0, state: TaskState::Empty }; MAX_TASKS];
        for i in 0..MAX_TASKS {
            let t = &(*sched).tasks[i];
            out[i] = TaskInfo {
                pid:      t.pid,
                name:     t.name,
                name_len: t.name_len,
                state:    t.state,
            };
        }
        out
    }
}

/// Spawn a task from a binary blob (flat `org 0x400000` or ELF64).
///
/// Finds a free slot, creates a per-process page table, maps code + stack,
/// and marks the task Ready.  Returns the 1-based PID on success.
pub unsafe fn spawn(code: &[u8], name: &str) -> Result<u8, &'static str> {
    if code.is_empty() { return Err("empty binary"); }

    let sched = &raw mut SCHED;

    // Find a free slot.
    let slot = (0..MAX_TASKS).find(|&i| {
        matches!((*sched).tasks[i].state, TaskState::Empty | TaskState::Dead(_))
    }).ok_or("max tasks reached (8)")?;

    // Create per-process page table (copies kernel higher-half entries).
    let cr3 = paging_allocator::create_user_page_table()
        .ok_or("OOM: cannot allocate page table")?;

    // Map user stack in the new page table.
    let stack_base = USER_STACK_TOP - (USER_STACK_PAGES * PAGE_SIZE) as u64;
    unsafe {
        paging_allocator::map_user_region_in(cr3, stack_base, USER_STACK_PAGES, true, false)
            .map_err(|_| "OOM: stack")?;
    }

    // Map code / load ELF — all into `cr3` without switching the kernel CR3.
    let entry = if crate::kernel::elf_loader::is_elf(code) {
        unsafe { crate::kernel::elf_loader::load_in(code, cr3)? }
    } else {
        let program_pages = code.len().div_ceil(PAGE_SIZE);
        unsafe {
            paging_allocator::map_user_region_in(
                cr3, USER_CODE_ADDR, program_pages, true, true)
                .map_err(|_| "OOM: code")?;
            paging_allocator::copy_to_region_in(cr3, USER_CODE_ADDR, code);
        }
        USER_CODE_ADDR
    };

    // Initialise the task slot.
    let pid  = (slot + 1) as u8;
    let task = &raw mut (*sched).tasks[slot];

    (*task).state           = TaskState::Ready;
    (*task).first_run       = true;
    (*task).ctx             = TaskContext::zeroed();
    (*task).entry           = entry;
    (*task).cr3             = cr3;
    (*task).pid             = pid;
    (*task).parent_pid      = 0;
    (*task).fs_base         = 0;
    (*task).heap_end        = 0;
    (*task).mmap_end        = 0;
    (*task).output_len          = 0;
    (*task).fd_table            = FdTable::new();
    (*task).pending_signals     = 0;
    (*task).signal_mask         = 0;
    (*task).saved_signal_mask   = 0;
    (*task).in_sigsuspend       = false;
    (*task).alarm_deadline      = 0;
    (*task).signal_handlers     = [0u64; NSIG];
    (*task).shm_attaches    = [const { crate::kernel::shm::ShmAttach::empty() }; crate::kernel::shm::MAX_ATTACH];
    (*task).mmap_regions    = [const { MmapRegion::empty() }; MAX_MMAP_REGIONS];
    (*task).mmap_nregions   = 0;

    // Map the signal-return trampoline page as writable so copy_to_region_in
    // can write to it in supervisor mode (CR0.WP faults on non-writable pages
    // regardless of privilege level).  User-writable is acceptable here.
    if paging_allocator::map_user_region_in(cr3, USER_SIGTRAMP, 1, true, true).is_ok() {
        paging_allocator::copy_to_region_in(cr3, USER_SIGTRAMP, SIGTRAMP_BYTES);
    }

    // Build the System V AMD64 argv block on the user stack (argv[0] = name).
    let initial_rsp = unsafe { write_argv_to_stack(cr3, USER_STACK_TOP, &[name]) };
    (*task).initial_rsp = initial_rsp;

    let bytes = name.as_bytes();
    let len   = bytes.len().min(16);
    let name_dst = core::ptr::addr_of_mut!((*task).name) as *mut u8;
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), name_dst, len); }
    (*task).name_len = len;

    unsafe {
        SERIAL_PORT.write_str("scheduler: spawned '");
        SERIAL_PORT.write_str(name);
        SERIAL_PORT.write_str("' pid=");
        SERIAL_PORT.write_decimal(pid as u32);
        SERIAL_PORT.write_str(" slot=");
        SERIAL_PORT.write_decimal(slot as u32);
        SERIAL_PORT.write_str(" entry=0x");
        SERIAL_PORT.write_hex((entry >> 32) as u32);
        SERIAL_PORT.write_hex(entry as u32);
        SERIAL_PORT.write_str("\n");
    }

    Ok(pid)
}

/// Advance the scheduler by one GUI frame (round-robin).
///
/// Wakes sleeping tasks, picks the next Ready task, runs it for one slice.
/// Returns `Some((pid, exit_code))` when a task permanently exits, else `None`.
pub unsafe fn tick() -> Option<(u8, i64)> {
    let sched = &raw mut SCHED;
    let now   = crate::kernel::timer::get_ticks();

    // Wake sleeping tasks.
    for i in 0..MAX_TASKS {
        if let TaskState::Sleeping(wake) = (*sched).tasks[i].state {
            if now >= wake { (*sched).tasks[i].state = TaskState::Ready; }
        }
    }

    // Fire SIGALRM for any task whose alarm has expired.
    for i in 0..MAX_TASKS {
        let deadline = (*sched).tasks[i].alarm_deadline;
        if deadline != 0 && now >= deadline {
            (*sched).tasks[i].alarm_deadline = 0;
            let pid = (*sched).tasks[i].pid;
            if pid != 0 { unsafe { send_signal(pid, SIGALRM) }; }
        }
    }

    // Wake tasks blocked on msgrcv_wait if their queue now has a message.
    for i in 0..MAX_TASKS {
        if let TaskState::WaitingForMsg(queue_id, msg_ptr) = (*sched).tasks[i].state {
            let mut msg = crate::kernel::ipc::Message::empty();
            if unsafe { crate::kernel::ipc::msgrcv(queue_id, &mut msg) } == 0 {
                unsafe {
                    core::ptr::write_unaligned(
                        msg_ptr as *mut crate::kernel::ipc::Message, msg);
                }
                (*sched).tasks[i].ctx.rax = 0; // success
                (*sched).tasks[i].state   = TaskState::Ready;
            }
        }
    }

    // Wake tasks waiting for a child that has died, and reap the child.
    for i in 0..MAX_TASKS {
        if let TaskState::Waiting(child_pid) = (*sched).tasks[i].state {
            for j in 0..MAX_TASKS {
                if (*sched).tasks[j].pid == child_pid {
                    if let TaskState::Dead(code) = (*sched).tasks[j].state {
                        (*sched).tasks[i].ctx.rax = code as u64;
                        (*sched).tasks[i].state   = TaskState::Ready;
                        (*sched).tasks[j].state   = TaskState::Empty;
                        (*sched).tasks[j].pid     = 0;
                        (*sched).tasks[j].parent_pid = 0;
                    }
                    break;
                }
            }
        }
    }

    // Round-robin: find the next Ready task starting after `current`.
    let start = ((*sched).current + 1) % MAX_TASKS;
    let mut chosen = None;
    for i in 0..MAX_TASKS {
        let idx = (start + i) % MAX_TASKS;
        if (*sched).tasks[idx].state == TaskState::Ready {
            chosen = Some(idx);
            break;
        }
    }

    let idx = chosen?;
    (*sched).current     = idx;
    CURRENT_TASK_IDX     = idx;

    // Deliver any pending signals before the task runs.
    if (*sched).tasks[idx].pending_signals != 0 {
        if unsafe { deliver_pending_signals(idx) } {
            // Task was killed by default action — reap it.
            let pid = (*sched).tasks[idx].pid;
            let cr3 = (*sched).tasks[idx].cr3;
            if cr3 != 0 {
                paging_allocator::free_user_page_table(cr3);
                (*sched).tasks[idx].cr3 = 0;
            }
            return Some((pid, (*sched).tasks[idx].state
                .exit_code().unwrap_or(-1)));
        }
    }

    (*sched).tasks[idx].state = TaskState::Running;
    (*sched).slice_remaining  = TICKS_PER_SLICE;

    let first  = (*sched).tasks[idx].first_run;
    let entry  = (*sched).tasks[idx].entry;
    let cr3    = (*sched).tasks[idx].cr3;

    let initial_rsp = (*sched).tasks[idx].initial_rsp;

    // Restore this task's FS_BASE MSR (thread-local storage pointer set by arch_prctl).
    let fs_base = (*sched).tasks[idx].fs_base;
    core::arch::asm!(
        "wrmsr",
        in("ecx") 0xC000_0100u32,  // IA32_FS_BASE
        in("eax") fs_base as u32,
        in("edx") (fs_base >> 32) as u32,
        options(nostack, nomem)
    );

    let exit_code = if first {
        (*sched).tasks[idx].first_run = false;
        crate::kernel::user_mode::launch_at(entry, initial_rsp, cr3)
    } else {
        let ctx_ptr = &raw const (*sched).tasks[idx].ctx;
        crate::kernel::user_mode::resume_user_context(&*ctx_ptr, cr3)
    };

    match exit_code {
        EXIT_PREEMPTED => {
            (*sched).tasks[idx].state = TaskState::Ready;
            None
        }
        EXIT_SLEEPING => None,
        code => {
            let pid        = (*sched).tasks[idx].pid;
            let parent_pid = (*sched).tasks[idx].parent_pid;
            (*sched).tasks[idx].state = TaskState::Dead(code);

            // Free user-space physical frames immediately — waitpid only needs the
            // exit code which is stored in the Dead variant.
            let cr3 = (*sched).tasks[idx].cr3;
            if cr3 != 0 {
                paging_allocator::free_user_page_table(cr3);
                (*sched).tasks[idx].cr3 = 0;
            }
            unsafe {
                SERIAL_PORT.write_str("scheduler: pid=");
                SERIAL_PORT.write_decimal(pid as u32);
                SERIAL_PORT.write_str(" '");
                let nlen = (*sched).tasks[idx].name_len;
                let mut nb = [0u8; 16];
                let nsrc = core::ptr::addr_of!((*sched).tasks[idx].name) as *const u8;
                core::ptr::copy_nonoverlapping(nsrc, nb.as_mut_ptr(), nlen);
                if let Ok(s) = core::str::from_utf8(&nb[..nlen]) {
                    SERIAL_PORT.write_str(s);
                }
                SERIAL_PORT.write_str("' exited (code ");
                SERIAL_PORT.write_decimal(code as u32);
                SERIAL_PORT.write_str(")\n");
            }

            // Deliver SIGCHLD to parent so bash/shells notice child exit.
            if parent_pid != 0 {
                send_signal(parent_pid, SIGCHLD);
            }

            Some((pid, code))
        }
    }
}

/// Called from the timer ISR when the running task's slice expires.
pub unsafe fn preempt(ctx: TaskContext) -> ! {
    let sched = &raw mut SCHED;
    let cur   = (*sched).current;
    (*sched).tasks[cur].ctx = ctx;
    crate::kernel::pic::send_eoi(0);
    crate::kernel::user_mode::exit_to_kernel(EXIT_PREEMPTED)
}

/// Called by the Sleep syscall.  Yields until `wake_tick`.
pub unsafe fn sleep_task(wake_tick: u64, mut ctx: TaskContext) -> ! {
    ctx.rax = 0;
    let sched = &raw mut SCHED;
    let cur   = (*sched).current;
    (*sched).tasks[cur].ctx   = ctx;
    (*sched).tasks[cur].state = TaskState::Sleeping(wake_tick);
    crate::kernel::user_mode::exit_to_kernel(EXIT_SLEEPING)
}

/// Send `signum` to the task with the given pid.
///
/// SIGKILL kills immediately; all other signals set a pending bit for delivery
/// before the next time the task runs.  Returns `false` if pid not found.
pub unsafe fn send_signal(pid: u8, signum: u8) -> bool {
    if signum == 0 || signum as usize >= NSIG { return false; }
    let sched = &raw mut SCHED;
    for i in 0..MAX_TASKS {
        let task = &raw mut (*sched).tasks[i];
        if (*task).pid != pid { continue; }
        if matches!((*task).state, TaskState::Empty | TaskState::Dead(_)) { break; }

        if signum == SIGKILL {
            // SIGKILL cannot be caught or ignored — kill immediately.
            (*task).state = TaskState::Dead(128 + signum as i64);
            return true;
        }

        // Set the pending bit.
        (*task).pending_signals |= 1u32 << (signum as u32 - 1);
        // If the task is sleeping, wake it so it can process the signal.
        if matches!((*task).state, TaskState::Sleeping(_)) {
            (*task).state = TaskState::Ready;
        }
        return true;
    }
    false
}

/// Forcibly terminate the task with the given pid (sends SIGKILL).
/// Kept for backward compatibility; callers can also use `send_signal`.
pub unsafe fn kill(pid: u8) -> bool {
    unsafe { send_signal(pid, SIGKILL) }
}

/// Deliver any pending signals for the task at `idx`.
///
/// Called from `tick()` just before running the task.
/// Returns `true` if the task was killed by a default-action signal.
unsafe fn deliver_pending_signals(idx: usize) -> bool {
    let sched = &raw mut SCHED;
    let task  = &raw mut (*sched).tasks[idx];

    // SIGKILL (9) and SIGSTOP (19) cannot be blocked; all other signals
    // respect signal_mask.
    let always_deliverable = (1u32 << (SIGKILL as u32 - 1)) | (1u32 << (SIGSTOP as u32 - 1));
    let deliverable = ((*task).pending_signals & !(*task).signal_mask)
                    | ((*task).pending_signals &  always_deliverable);
    if deliverable == 0 { return false; }

    // Work only on deliverable bits; leave masked signals in pending_signals.
    let mut to_deliver = deliverable;
    (*task).pending_signals &= !deliverable; // will be restored if handler re-queues

    while to_deliver != 0 {
        // Find lowest set bit (signal number = bit position + 1).
        let bit    = to_deliver.trailing_zeros();
        let signum = (bit + 1) as u8;
        to_deliver &= !(1u32 << bit);

        let handler = if (signum as usize) < NSIG {
            (*task).signal_handlers[signum as usize]
        } else {
            SIG_DFL
        };

        if handler == SIG_IGN {
            continue; // explicitly ignored
        }

        if handler == SIG_DFL {
            // Default action: most signals terminate the process.
            match signum {
                SIGCHLD | SIGCONT => continue, // default = ignore
                _ => {
                    (*task).state = TaskState::Dead(128 + signum as i64);
                    return true;
                }
            }
        }

        // User-defined handler: set up a signal frame on the user stack.
        let cr3 = (*task).cr3;
        let rsp = (*task).ctx.rsp;

        // Push trampoline address (return address for the handler).
        let rsp = rsp - 8;
        let tramp = USER_SIGTRAMP;
        paging_allocator::copy_to_region_in(
            cr3, rsp, &tramp.to_ne_bytes());

        // Push SignalFrame below that.
        let frame = SignalFrame {
            rip:    (*task).ctx.rip,
            rax:    (*task).ctx.rax,
            rbx:    (*task).ctx.rbx,
            rcx:    (*task).ctx.rcx,
            rdx:    (*task).ctx.rdx,
            rsi:    (*task).ctx.rsi,
            rdi:    (*task).ctx.rdi,
            r8:     (*task).ctx.r8,
            r9:     (*task).ctx.r9,
            r10:    (*task).ctx.r10,
            r11:    (*task).ctx.r11,
            r12:    (*task).ctx.r12,
            r13:    (*task).ctx.r13,
            r14:    (*task).ctx.r14,
            r15:    (*task).ctx.r15,
            rbp:    (*task).ctx.rbp,
            rflags: (*task).ctx.rflags,
        };
        let frame_size = core::mem::size_of::<SignalFrame>() as u64;
        let rsp = rsp - frame_size;
        // Align to 16 bytes as required by the System V ABI.
        let rsp = rsp & !0xF;

        let frame_bytes = core::slice::from_raw_parts(
            &frame as *const SignalFrame as *const u8,
            frame_size as usize,
        );
        paging_allocator::copy_to_region_in(cr3, rsp, frame_bytes);

        // Redirect execution to the handler.
        (*task).ctx.rsp = rsp;
        (*task).ctx.rip = handler;
        (*task).ctx.rdi = signum as u64; // first argument: signal number

        // Only deliver one signal per tick to avoid stack overflow.
        break;
    }
    false
}

/// Create a child process that is a full copy of the task at `parent_idx`.
///
/// `child_ctx` is the register snapshot to use for the child (caller sets
/// `rax = 0` so the child returns 0 from `fork`).  Returns the child's PID
/// on success.
pub unsafe fn fork_task(
    parent_idx: usize,
    child_ctx:  crate::kernel::user_mode::TaskContext,
) -> Result<u8, &'static str> {
    let sched = &raw mut SCHED;

    // Find a free slot (any slot other than the parent's).
    let child_slot = (0..MAX_TASKS)
        .find(|&i| i != parent_idx
            && matches!((*sched).tasks[i].state, TaskState::Empty | TaskState::Dead(_)))
        .ok_or("max tasks reached")?;

    let parent_cr3 = (*sched).tasks[parent_idx].cr3;

    // Deep-copy the parent's address space.
    let child_cr3 = unsafe { paging_allocator::copy_user_page_table(parent_cr3) }
        .ok_or("OOM: fork page table")?;

    let child_pid  = (child_slot + 1) as u8;
    let parent_pid = (*sched).tasks[parent_idx].pid;

    // Copy all task fields from parent; override the child-specific ones.
    let parent_fd       = (*sched).tasks[parent_idx].fd_table;
    let parent_heap     = (*sched).tasks[parent_idx].heap_end;
    let parent_mmap     = (*sched).tasks[parent_idx].mmap_end;
    let parent_mregions = (*sched).tasks[parent_idx].mmap_regions;
    let parent_nregions = (*sched).tasks[parent_idx].mmap_nregions;
    let parent_entry   = (*sched).tasks[parent_idx].entry;
    let parent_name    = (*sched).tasks[parent_idx].name;
    let parent_nlen    = (*sched).tasks[parent_idx].name_len;
    let parent_cwd     = (*sched).tasks[parent_idx].cwd;
    let parent_cwdl    = (*sched).tasks[parent_idx].cwd_len;
    let parent_sighand = (*sched).tasks[parent_idx].signal_handlers;

    let child = &raw mut (*sched).tasks[child_slot];
    (*child).state      = TaskState::Ready;
    (*child).ctx        = child_ctx;
    (*child).first_run  = false;   // resume via context restore
    (*child).entry      = parent_entry;
    (*child).cr3        = child_cr3;
    (*child).pid        = child_pid;
    (*child).parent_pid = parent_pid;
    (*child).pgid       = (*sched).tasks[parent_idx].pgid; // inherit parent's pgid
    (*child).heap_end   = parent_heap;
    (*child).mmap_end   = parent_mmap;
    (*child).output_len = 0;
    (*child).fd_table   = parent_fd;
    (*child).cwd             = parent_cwd;
    (*child).cwd_len         = parent_cwdl;
    // Children inherit signal handlers but start with clean pending mask, mask, alarm, and shm.
    (*child).pending_signals   = 0;
    (*child).signal_mask       = 0;
    (*child).saved_signal_mask = 0;
    (*child).in_sigsuspend     = false;
    (*child).alarm_deadline    = 0;
    (*child).signal_handlers   = parent_sighand;
    (*child).shm_attaches    = [const { crate::kernel::shm::ShmAttach::empty() }; crate::kernel::shm::MAX_ATTACH];
    (*child).mmap_regions    = parent_mregions;
    (*child).mmap_nregions   = parent_nregions;
    // Addref every pipe end the child inherited so reference counts stay correct.
    for slot in &(*child).fd_table.entries {
        if let Some(e) = slot {
            if e.backend == crate::kernel::fs::ramfs::FdBackend::Pipe {
                crate::kernel::pipe::addref(e.raw_fd);
            }
        }
    }
    (*child).name       = parent_name;
    (*child).name_len   = parent_nlen;

    unsafe {
        SERIAL_PORT.write_str("scheduler: fork parent=");
        SERIAL_PORT.write_decimal(parent_pid as u32);
        SERIAL_PORT.write_str(" child=");
        SERIAL_PORT.write_decimal(child_pid as u32);
        SERIAL_PORT.write_str(" slot=");
        SERIAL_PORT.write_decimal(child_slot as u32);
        SERIAL_PORT.write_str("\n");
    }

    Ok(child_pid)
}

/// Block the task at `parent_idx` until the child with `child_pid` dies.
///
/// Sets the parent's state to `Waiting(child_pid)`, saves its context, then
/// jumps back to the scheduler via `exit_to_kernel(EXIT_SLEEPING)`.
pub unsafe fn wait_for_pid(
    parent_idx: usize,
    child_pid:  u8,
    mut ctx:    crate::kernel::user_mode::TaskContext,
) -> ! {
    ctx.rax = 0; // will be overwritten with the exit code on wakeup
    let sched = &raw mut SCHED;
    (*sched).tasks[parent_idx].ctx   = ctx;
    (*sched).tasks[parent_idx].state = TaskState::Waiting(child_pid);
    unsafe { crate::kernel::user_mode::exit_to_kernel(EXIT_SLEEPING) }
}

/// Block the current task until a message arrives on `queue_id`.
///
/// On wakeup, `tick()` will have already written the message to `msg_ptr`
/// and set `rax = 0`.  If the queue already has a message the task is placed
/// in WaitingForMsg and will be woken on the very next tick.
pub unsafe fn wait_for_msg(
    queue_id: u32,
    msg_ptr:  u64,
    mut ctx:  crate::kernel::user_mode::TaskContext,
) -> ! {
    ctx.rax = 0;
    let sched = &raw mut SCHED;
    let cur   = (*sched).current;
    (*sched).tasks[cur].ctx   = ctx;
    (*sched).tasks[cur].state = TaskState::WaitingForMsg(queue_id, msg_ptr);
    unsafe { crate::kernel::user_mode::exit_to_kernel(EXIT_SLEEPING) }
}

/// Returns `true` when at least one non-finished task exists.
pub fn has_task() -> bool {
    unsafe {
        let sched = &raw const SCHED;
        (0..MAX_TASKS).any(|i| !matches!((*sched).tasks[i].state,
            TaskState::Empty | TaskState::Dead(_)))
    }
}

/// Count of tasks currently Ready, Running, Sleeping, or Waiting.
pub fn task_count() -> usize {
    unsafe {
        let sched = &raw const SCHED;
        (0..MAX_TASKS).filter(|&i| matches!((*sched).tasks[i].state,
            TaskState::Ready | TaskState::Running
            | TaskState::Sleeping(_) | TaskState::Waiting(_)
            | TaskState::WaitingForMsg(_, _))).count()
    }
}
