//! Ring-3 bootstrap and user-program loader for OxideOS.
//!
//! Supports loading flat binaries from the kernel's program registry and
//! capturing their stdout output so it can be displayed in the GUI terminal.

use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use crate::kernel::paging_allocator;
use crate::kernel::serial::SERIAL_PORT;

// ── TaskContext ────────────────────────────────────────────────────────────
// Saved register state for a user task. Offsets must match the assembly in
// resume_user_context_trampoline exactly.
//
//   r15    +0    r14    +8    r13   +16   r12   +24
//   r11   +32    r10   +40    r9   +48    r8   +56
//   rdi   +64    rsi   +72   rbp   +80   rdx   +88
//   rcx   +96    rbx  +104   rax  +112
//   rip  +120    cs   +128  rflags+136   rsp  +144   ss +152

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskContext {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9:  u64, pub r8:  u64,
    pub rdi: u64, pub rsi: u64, pub rbp: u64, pub rdx: u64,
    pub rcx: u64, pub rbx: u64, pub rax: u64,
    pub rip: u64, pub cs:  u64, pub rflags: u64, pub rsp: u64, pub ss: u64,
}

impl TaskContext {
    pub const fn zeroed() -> Self {
        Self {
            r15:0, r14:0, r13:0, r12:0, r11:0, r10:0, r9:0, r8:0,
            rdi:0, rsi:0, rbp:0, rdx:0, rcx:0, rbx:0, rax:0,
            rip:0, cs:0, rflags:0, rsp:0, ss:0,
        }
    }
}

/// Saved syscall-entry context for the currently executing syscall.
/// Set by handle_system_call() before dispatching; used by sleep to yield.
pub static mut CURRENT_SYSCALL_CTX: Option<TaskContext> = None;

const PAGE_SIZE: usize = 4096;
const USER_CODE_ADDR: u64 = 0x0040_0000;
const USER_STACK_TOP: u64 = 0x0080_0000;
const USER_STACK_PAGES: usize = 4;

// ── Output capture ─────────────────────────────────────────────────────────
// Routed to per-task buffers in the scheduler.

/// Append bytes to the currently-running task's output buffer.
pub fn output_write(bytes: &[u8]) {
    let idx = unsafe { crate::kernel::scheduler::CURRENT_TASK_IDX };
    crate::kernel::scheduler::output_write_for_task(idx, bytes);
}
const USER_PROGRAM: [u8; 23] = [
    0x48, 0xC7, 0xC0, 0x28, 0x00, 0x00, 0x00,
    0xCD, 0x80,
    0x48, 0x89, 0xC7,
    0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,
    0xCD, 0x80,
    0xEB, 0xFE,
];

#[repr(C)]
#[derive(Clone, Copy)]
struct SavedKernelContext {
    rsp: u64,
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rflags: u64,
}

static USER_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);
static USER_EXIT_CODE: AtomicI64 = AtomicI64::new(-1);
/// The kernel CR3 saved immediately before we switch to a user page table.
/// Restored by exit_to_kernel() before returning to the scheduler.
static mut KERNEL_CR3: u64 = 0;
#[unsafe(no_mangle)]
static mut RETURN_CONTEXT: SavedKernelContext = SavedKernelContext {
    rsp: 0,
    rbx: 0,
    rbp: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
    rflags: 0,
};

global_asm!(
    r#"
.intel_syntax noprefix

.section .text.enter_user_mode, "ax", @progbits
.global enter_user_mode_trampoline
enter_user_mode_trampoline:
    mov [rip + RETURN_CONTEXT + 0],  rsp
    mov [rip + RETURN_CONTEXT + 8],  rbx
    mov [rip + RETURN_CONTEXT + 16], rbp
    mov [rip + RETURN_CONTEXT + 24], r12
    mov [rip + RETURN_CONTEXT + 32], r13
    mov [rip + RETURN_CONTEXT + 40], r14
    mov [rip + RETURN_CONTEXT + 48], r15
    pushfq
    pop rax
    mov [rip + RETURN_CONTEXT + 56], rax

    push 0x1b        /* ss  (user data selector) */
    push rsi         /* rsp (user stack top)     */
    pushfq
    pop rax
    or rax, 0x200    /* enable IF in rflags      */
    push rax
    push 0x23        /* cs  (user code selector) */
    push rdi         /* rip (entry point)        */
    iretq

/* ── resume_user_context_trampoline ──────────────────────────────────────
 * Resumes a previously saved TaskContext (e.g. after preemption or sleep).
 * rdi = *const TaskContext
 *
 * Offsets match TaskContext layout (see user_mode.rs):
 *   r15+0  r14+8  r13+16 r12+24 r11+32 r10+40 r9+48  r8+56
 *   rdi+64 rsi+72 rbp+80 rdx+88 rcx+96 rbx+104 rax+112
 *   rip+120 cs+128 rflags+136 rsp+144 ss+152
 */
.global resume_user_context_trampoline
resume_user_context_trampoline:
    /* Save kernel callee-saved state so exit_to_kernel can return here. */
    mov [rip + RETURN_CONTEXT + 0],  rsp
    mov [rip + RETURN_CONTEXT + 8],  rbx
    mov [rip + RETURN_CONTEXT + 16], rbp
    mov [rip + RETURN_CONTEXT + 24], r12
    mov [rip + RETURN_CONTEXT + 32], r13
    mov [rip + RETURN_CONTEXT + 40], r14
    mov [rip + RETURN_CONTEXT + 48], r15
    pushfq
    pop rax
    mov [rip + RETURN_CONTEXT + 56], rax

    /* Push iretq frame: ss rsp rflags cs rip (pushed high→low). */
    push QWORD PTR [rdi + 152]   /* ss     */
    push QWORD PTR [rdi + 144]   /* rsp    */
    push QWORD PTR [rdi + 136]   /* rflags */
    push QWORD PTR [rdi + 128]   /* cs     */
    push QWORD PTR [rdi + 120]   /* rip    */

    /* Restore GPRs — rdi is the base pointer so restore it last. */
    mov r15, [rdi + 0]
    mov r14, [rdi + 8]
    mov r13, [rdi + 16]
    mov r12, [rdi + 24]
    mov r11, [rdi + 32]
    mov r10, [rdi + 40]
    mov r9,  [rdi + 48]
    mov r8,  [rdi + 56]
    mov rsi, [rdi + 72]
    mov rbp, [rdi + 80]
    mov rdx, [rdi + 88]
    mov rcx, [rdi + 96]
    mov rbx, [rdi + 104]
    mov rax, [rdi + 112]
    mov rdi, [rdi + 64]   /* must be last */
    iretq

.att_syntax prefix
"#
);

unsafe extern "C" {
    fn enter_user_mode_trampoline(entry: u64, stack_top: u64) -> i64;
    fn resume_user_context_trampoline(ctx: *const TaskContext) -> i64;
}

pub fn is_active() -> bool {
    USER_MODE_ACTIVE.load(Ordering::Relaxed)
}

/// Set USER_MODE_ACTIVE flag (called by scheduler before each task slice).
pub fn set_active(v: bool) {
    USER_MODE_ACTIVE.store(v, Ordering::Relaxed);
}

/// Run a user task for the first time, entering at `entry` with `stack_top`.
/// Switches to `cr3` (per-process page table) before entering ring 3.
pub unsafe fn launch_at(entry: u64, stack_top: u64, cr3: u64) -> i64 {
    unsafe {
        // Save kernel CR3 so exit_to_kernel can restore it.
        core::arch::asm!("mov {}, cr3", out(reg) KERNEL_CR3, options(nostack, nomem));
        core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, nomem));
    }
    enter_user_mode(entry, stack_top)
}

/// Resume a previously saved task context (e.g. after preemption or sleep).
/// Switches to `cr3` before returning to ring 3.
pub unsafe fn resume_user_context(ctx: &TaskContext, cr3: u64) -> i64 {
    unsafe {
        // Save kernel CR3 so exit_to_kernel can restore it.
        core::arch::asm!("mov {}, cr3", out(reg) KERNEL_CR3, options(nostack, nomem));
        core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, nomem));
    }
    USER_MODE_ACTIVE.store(true, Ordering::Relaxed);
    resume_user_context_trampoline(ctx as *const TaskContext)
}

pub unsafe fn exit_to_kernel(code: i64) -> ! {
    // Restore the kernel page table FIRST so that all subsequent kernel code
    // (including free_user_page_table) runs with a valid, stable CR3.
    let kcr3 = unsafe { KERNEL_CR3 };
    if kcr3 != 0 {
        unsafe { core::arch::asm!("mov cr3, {}", in(reg) kcr3, options(nostack, nomem)); }
    }

    CURRENT_SYSCALL_CTX = None;
    USER_EXIT_CODE.store(code, Ordering::Relaxed);
    USER_MODE_ACTIVE.store(false, Ordering::Relaxed);

    let ctx = RETURN_CONTEXT;
    asm!(
        "mov rsp, {saved_rsp}",
        "mov rbx, {saved_rbx}",
        "mov rbp, {saved_rbp}",
        "mov r12, {saved_r12}",
        "mov r13, {saved_r13}",
        "mov r14, {saved_r14}",
        "mov r15, {saved_r15}",
        "push {saved_rflags}",
        "popfq",
        "mov rax, {exit_code}",
        "ret",
        saved_rsp = in(reg) ctx.rsp,
        saved_rbx = in(reg) ctx.rbx,
        saved_rbp = in(reg) ctx.rbp,
        saved_r12 = in(reg) ctx.r12,
        saved_r13 = in(reg) ctx.r13,
        saved_r14 = in(reg) ctx.r14,
        saved_r15 = in(reg) ctx.r15,
        saved_rflags = in(reg) ctx.rflags,
        exit_code = in(reg) code,
        options(noreturn)
    );
}

unsafe fn enter_user_mode(entry: u64, stack_top: u64) -> i64 {
    USER_EXIT_CODE.store(-1, Ordering::Relaxed);
    USER_MODE_ACTIVE.store(true, Ordering::Relaxed);
    enter_user_mode_trampoline(entry, stack_top)
}

/// Load a flat binary into the user code region and execute it in ring 3.
/// Pages are mapped on the first call; subsequent calls reuse the mapping and
/// overwrite the code in place (safe because there is only one user task).
pub unsafe fn load_and_run(code: &[u8]) -> i64 {
    let program_pages = code.len().div_ceil(PAGE_SIZE);
    let stack_base    = USER_STACK_TOP - (USER_STACK_PAGES * PAGE_SIZE) as u64;

    // Map regions — silently ignore "already mapped" on re-runs.
    let _ = paging_allocator::map_user_region(USER_CODE_ADDR, program_pages, true, true);
    let _ = paging_allocator::map_user_region(stack_base, USER_STACK_PAGES, true, false);

    // Overwrite the code region with the new program.
    paging_allocator::copy_to_region(USER_CODE_ADDR, code);

    SERIAL_PORT.write_str("Launching user program (ring 3)...\n");
    enter_user_mode(USER_CODE_ADDR, USER_STACK_TOP - 16)
}

pub unsafe fn run_demo() -> i64 {
    let program = &USER_PROGRAM;
    let program_pages = program.len().div_ceil(PAGE_SIZE);
    let stack_base = USER_STACK_TOP - (USER_STACK_PAGES * PAGE_SIZE) as u64;

    SERIAL_PORT.write_str("Preparing ring-3 demo program...\n");
    // Step 2 aims to prove the ring-3 transition path first. We keep the code
    // mapping writable during load for now and can split load-vs-exec
    // permissions in the next milestone.
    paging_allocator::map_user_region(USER_CODE_ADDR, program_pages, true, true)
        .expect("failed to map user code");
    paging_allocator::copy_to_region(USER_CODE_ADDR, program);

    paging_allocator::map_user_region(stack_base, USER_STACK_PAGES, true, false)
        .expect("failed to map user stack");

    SERIAL_PORT.write_str("Entering ring 3 demo...\n");
    enter_user_mode(USER_CODE_ADDR, USER_STACK_TOP - 16)
}
