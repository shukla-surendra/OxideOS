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
const  USER_STACK_PAGES:   usize = 4;
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

pub const CWD_MAX: usize = 128;

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
    /// Current userspace heap break (virtual address).  0 = unset (use USER_HEAP_BASE).
    pub heap_end:   u64,
    pub output:     [u8; TASK_OUTPUT_CAP],
    pub output_len: usize,
    /// Per-process open file-descriptor table.
    /// FDs 0/1/2 (stdin/stdout/stderr) are reserved; real files start at FD 3.
    pub fd_table:   FdTable,
    /// Current working directory (null-terminated UTF-8 path).
    pub cwd:        [u8; CWD_MAX],
    pub cwd_len:    usize,
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
            heap_end:   0,
            output:     [0u8; TASK_OUTPUT_CAP],
            output_len: 0,
            fd_table:   FdTable::new(),
            cwd,
            cwd_len:    1, // "/"
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

    (*task).state      = TaskState::Ready;
    (*task).first_run  = true;
    (*task).ctx        = TaskContext::zeroed();
    (*task).entry      = entry;
    (*task).cr3        = cr3;
    (*task).pid        = pid;
    (*task).parent_pid = 0;
    (*task).heap_end   = 0;
    (*task).output_len = 0;
    (*task).fd_table   = FdTable::new();

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
    (*sched).tasks[idx].state = TaskState::Running;
    (*sched).slice_remaining  = TICKS_PER_SLICE;

    let first  = (*sched).tasks[idx].first_run;
    let entry  = (*sched).tasks[idx].entry;
    let cr3    = (*sched).tasks[idx].cr3;

    let exit_code = if first {
        (*sched).tasks[idx].first_run = false;
        crate::kernel::user_mode::launch_at(entry, USER_STACK_TOP - 16, cr3)
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
            let pid = (*sched).tasks[idx].pid;
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

/// Forcibly terminate the task with the given pid.  Returns `false` if not found.
pub unsafe fn kill(pid: u8) -> bool {
    let sched = &raw mut SCHED;
    for i in 0..MAX_TASKS {
        if (*sched).tasks[i].pid == pid
            && !matches!((*sched).tasks[i].state, TaskState::Empty | TaskState::Dead(_))
        {
            (*sched).tasks[i].state = TaskState::Dead(-1);
            return true;
        }
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
    let parent_fd    = (*sched).tasks[parent_idx].fd_table;
    let parent_heap  = (*sched).tasks[parent_idx].heap_end;
    let parent_entry = (*sched).tasks[parent_idx].entry;
    let parent_name  = (*sched).tasks[parent_idx].name;
    let parent_nlen  = (*sched).tasks[parent_idx].name_len;
    let parent_cwd   = (*sched).tasks[parent_idx].cwd;
    let parent_cwdl  = (*sched).tasks[parent_idx].cwd_len;

    let child = &raw mut (*sched).tasks[child_slot];
    (*child).state      = TaskState::Ready;
    (*child).ctx        = child_ctx;
    (*child).first_run  = false;   // resume via context restore
    (*child).entry      = parent_entry;
    (*child).cr3        = child_cr3;
    (*child).pid        = child_pid;
    (*child).parent_pid = parent_pid;
    (*child).heap_end   = parent_heap;
    (*child).output_len = 0;
    (*child).fd_table   = parent_fd;
    (*child).cwd        = parent_cwd;
    (*child).cwd_len    = parent_cwdl;
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
