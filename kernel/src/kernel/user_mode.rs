//! Minimal ring-3 bootstrap for OxideOS.
//!
//! This module runs a single hard-coded user program in the current address
//! space. It is intentionally a first milestone, not a full process loader.

use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use crate::kernel::paging_allocator;
use crate::kernel::serial::SERIAL_PORT;

const PAGE_SIZE: usize = 4096;
const USER_CODE_ADDR: u64 = 0x0040_0000;
const USER_STACK_TOP: u64 = 0x0080_0000;
const USER_STACK_PAGES: usize = 4;
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
}

static USER_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);
static USER_EXIT_CODE: AtomicI64 = AtomicI64::new(-1);
#[unsafe(no_mangle)]
static mut RETURN_CONTEXT: SavedKernelContext = SavedKernelContext {
    rsp: 0,
    rbx: 0,
    rbp: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
};

global_asm!(
    r#"
.intel_syntax noprefix

.section .text.enter_user_mode, "ax", @progbits
.global enter_user_mode_trampoline
enter_user_mode_trampoline:
    mov [rip + RETURN_CONTEXT + 0], rsp
    mov [rip + RETURN_CONTEXT + 8], rbx
    mov [rip + RETURN_CONTEXT + 16], rbp
    mov [rip + RETURN_CONTEXT + 24], r12
    mov [rip + RETURN_CONTEXT + 32], r13
    mov [rip + RETURN_CONTEXT + 40], r14
    mov [rip + RETURN_CONTEXT + 48], r15

    push 0x1b
    push rsi
    pushfq
    pop rax
    or rax, 0x200
    push rax
    push 0x23
    push rdi
    iretq
.att_syntax prefix
"#
);

unsafe extern "C" {
    fn enter_user_mode_trampoline(entry: u64, stack_top: u64) -> i64;
}

pub fn is_active() -> bool {
    USER_MODE_ACTIVE.load(Ordering::Relaxed)
}

pub unsafe fn exit_to_kernel(code: i64) -> ! {
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
        "mov rax, {exit_code}",
        "ret",
        saved_rsp = in(reg) ctx.rsp,
        saved_rbx = in(reg) ctx.rbx,
        saved_rbp = in(reg) ctx.rbp,
        saved_r12 = in(reg) ctx.r12,
        saved_r13 = in(reg) ctx.r13,
        saved_r14 = in(reg) ctx.r14,
        saved_r15 = in(reg) ctx.r15,
        exit_code = in(reg) code,
        options(noreturn)
    );
}

unsafe fn enter_user_mode(entry: u64, stack_top: u64) -> i64 {
    USER_EXIT_CODE.store(-1, Ordering::Relaxed);
    USER_MODE_ACTIVE.store(true, Ordering::Relaxed);
    enter_user_mode_trampoline(entry, stack_top)
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
