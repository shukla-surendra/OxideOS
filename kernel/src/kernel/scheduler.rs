//! Simple preemptive scheduler for OxideOS.
//!
//! Supports a single concurrent user-mode task. The timer ISR preempts
//! the running task after `TICKS_PER_SLICE` timer ticks, saving register
//! state into the task slot. The kernel GUI loop renders one frame then
//! calls `tick()` to resume the task.
//!
//! # Memory layout
//! All programs are assembled at `org 0x400000`, so only one task can live
//! in the address space at a time. Multi-task support requires per-process
//! CR3 switching (future work).

use crate::kernel::paging_allocator;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::user_mode::TaskContext;

const PAGE_SIZE: usize = 4096;
const USER_CODE_ADDR: u64 = 0x0040_0000;
const USER_STACK_TOP: u64 = 0x0080_0000;
const USER_STACK_PAGES: usize = 4;

/// Timer ticks a task runs before being preempted (100 Hz → 20 ms).
pub const TICKS_PER_SLICE: u64 = 2;

/// Sentinel returned by exit_to_kernel when the timer ISR preempts a task.
pub const EXIT_PREEMPTED: i64 = i64::MIN;

/// Sentinel returned by exit_to_kernel when a task calls sleep().
pub const EXIT_SLEEPING: i64 = i64::MIN + 1;

// ── Task state ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TaskState {
    Empty,
    Ready,
    Running,
    Sleeping(u64), // wake at this tick count
    Dead(i64),     // exit code
}

pub struct Task {
    pub state:      TaskState,
    pub ctx:        TaskContext,
    pub name:       [u8; 16],
    pub name_len:   usize,
    /// True on the first run — use enter_user_mode path, not resume.
    pub first_run:  bool,
    /// Entry point (may differ from USER_CODE_ADDR for ELF binaries).
    pub entry:      u64,
}

impl Task {
    const fn empty() -> Self {
        Self {
            state:    TaskState::Empty,
            ctx:      TaskContext::zeroed(),
            name:     [0u8; 16],
            name_len: 0,
            first_run: true,
            entry:    USER_CODE_ADDR,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }
}

// ── Scheduler state ────────────────────────────────────────────────────────

pub struct Scheduler {
    pub task:            Task,
    pub slice_remaining: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            task:            Task::empty(),
            slice_remaining: 0,
        }
    }
}

pub static mut SCHED: Scheduler = Scheduler::new();

// ── Public API ─────────────────────────────────────────────────────────────

/// Spawn a task from a binary blob (flat `org 0x400000` or ELF64).
///
/// Automatically detects ELF by magic bytes. For ELF binaries the loader
/// maps every PT_LOAD segment; the entry point comes from `e_entry`.
/// For flat binaries the code is copied verbatim to `USER_CODE_ADDR`.
///
/// Any currently running task is replaced. The output capture buffer is
/// cleared so the new task starts with a clean slate.
pub unsafe fn spawn(code: &[u8], name: &str) -> Result<(), &'static str> {
    if code.is_empty() {
        return Err("empty binary");
    }

    let stack_base = USER_STACK_TOP - (USER_STACK_PAGES * PAGE_SIZE) as u64;
    let _ = paging_allocator::map_user_region(stack_base, USER_STACK_PAGES, true, false);

    let entry = if crate::kernel::elf_loader::is_elf(code) {
        // ELF path: loader maps its own segments.
        unsafe { crate::kernel::elf_loader::load(code)? }
    } else {
        // Flat binary path.
        let program_pages = code.len().div_ceil(PAGE_SIZE);
        let _ = paging_allocator::map_user_region(USER_CODE_ADDR, program_pages, true, true);
        paging_allocator::copy_to_region(USER_CODE_ADDR, code);
        USER_CODE_ADDR
    };

    crate::kernel::user_mode::output_clear();

    let sched = &raw mut SCHED;
    (*sched).task.state     = TaskState::Ready;
    (*sched).task.first_run = true;
    (*sched).task.ctx       = TaskContext::zeroed();
    (*sched).task.entry     = entry;

    let bytes = name.as_bytes();
    let len   = bytes.len().min(16);
    let name_dst = core::ptr::addr_of_mut!((*sched).task.name) as *mut u8;
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), name_dst, len);
    (*sched).task.name_len = len;

    SERIAL_PORT.write_str("scheduler: spawned '");
    SERIAL_PORT.write_str(name);
    SERIAL_PORT.write_str("' entry=0x");
    SERIAL_PORT.write_hex((entry >> 32) as u32);
    SERIAL_PORT.write_hex(entry as u32);
    SERIAL_PORT.write_str("\n");

    Ok(())
}

/// Advance the scheduler by one GUI frame.
///
/// If a task is `Ready` or has woken from sleep, it is resumed for one
/// time slice. Blocks until the task is preempted, exits, or sleeps.
///
/// Returns `Some(exit_code)` the frame the task permanently exits,
/// `None` otherwise (preempted, sleeping, or no task).
pub unsafe fn tick() -> Option<i64> {
    let sched = &raw mut SCHED;

    // Wake sleeping tasks.
    if let TaskState::Sleeping(wake_tick) = (*sched).task.state {
        if crate::kernel::timer::get_ticks() >= wake_tick {
            (*sched).task.state = TaskState::Ready;
        }
    }

    if (*sched).task.state != TaskState::Ready {
        return None;
    }

    (*sched).task.state      = TaskState::Running;
    (*sched).slice_remaining = TICKS_PER_SLICE;

    let first_run = (*sched).task.first_run;
    let exit_code = if first_run {
        (*sched).task.first_run = false;
        let entry = (*sched).task.entry;
        crate::kernel::user_mode::launch_at(entry, USER_STACK_TOP - 16)
    } else {
        let ctx_ptr = &raw const (*sched).task.ctx;
        crate::kernel::user_mode::resume_user_context(&*ctx_ptr)
    };

    // Control returns here via exit_to_kernel restoring RETURN_CONTEXT.
    match exit_code {
        EXIT_PREEMPTED => {
            // Context already saved in task.ctx by preempt().
            (*sched).task.state = TaskState::Ready;
            None
        }
        EXIT_SLEEPING => {
            // task.state already set to Sleeping(wake_tick) by sleep_task().
            None
        }
        code => {
            (*sched).task.state = TaskState::Dead(code);
            SERIAL_PORT.write_str("scheduler: '");
            // Copy name to stack to avoid holding a ref to the static.
            let name_len = (*sched).task.name_len;
            let mut name_buf = [0u8; 16];
            let name_src = core::ptr::addr_of!((*sched).task.name) as *const u8;
            core::ptr::copy_nonoverlapping(name_src, name_buf.as_mut_ptr(), name_len);
            let name_str = core::str::from_utf8(&name_buf[..name_len]).unwrap_or("?");
            SERIAL_PORT.write_str(name_str);
            SERIAL_PORT.write_str("' exited (code ");
            SERIAL_PORT.write_decimal(code as u32);
            SERIAL_PORT.write_str(")\n");
            Some(code)
        }
    }
}

/// Called from the timer ISR when a ring-3 task's time slice expires.
///
/// Saves `ctx` into the task slot and jumps back to the kernel via
/// `exit_to_kernel`. Sends PIC EOI for IRQ0 before jumping so it is not
/// lost when the normal ISR return path is bypassed.
pub unsafe fn preempt(ctx: TaskContext) -> ! {
    let sched = &raw mut SCHED;
    (*sched).task.ctx = ctx;
    crate::kernel::pic::send_eoi(0);
    crate::kernel::user_mode::exit_to_kernel(EXIT_PREEMPTED)
}

/// Called from the Sleep syscall dispatcher to yield until `wake_tick`.
///
/// `ctx` must be the task's saved register state at the moment of the
/// syscall (from `CURRENT_SYSCALL_CTX`). `rax` is overwritten with 0 so
/// that sleep returns 0 to user code when resumed.
pub unsafe fn sleep_task(wake_tick: u64, mut ctx: TaskContext) -> ! {
    ctx.rax = 0; // sleep returns 0 to the caller
    let sched = &raw mut SCHED;
    (*sched).task.ctx   = ctx;
    (*sched).task.state = TaskState::Sleeping(wake_tick);
    crate::kernel::user_mode::exit_to_kernel(EXIT_SLEEPING)
}

/// Returns true if a runnable (non-Empty, non-Dead) task exists.
pub fn has_task() -> bool {
    unsafe {
        let sched = &raw const SCHED;
        !matches!((*sched).task.state, TaskState::Empty | TaskState::Dead(_))
    }
}
