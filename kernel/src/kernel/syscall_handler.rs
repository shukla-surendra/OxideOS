// src/kernel/syscall_handler.rs
//! SYSCALL/SYSRET fast-path for OxideOS.
//!
//! musl-libc uses the `syscall` instruction, not `int 0x80`. This module
//! wires up LSTAR/STAR/FMASK so that `syscall` lands in `syscall_entry`.
//!
//! STAR layout (for our GDT):
//!   GDT[0]=null  GDT[1]=kernel-code(0x08)  GDT[2]=kernel-data(0x10)
//!   GDT[3]=user-data(0x18,DPL=3)  GDT[4]=user-code(0x20,DPL=3)
//!   GDT[5..6]=TSS(0x28)
//!
//!   SYSRET (64-bit) loads:
//!     CS ← STAR[63:48] + 16, RPL forced to 3
//!     SS ← STAR[63:48] + 8,  RPL forced to 3
//!
//!   To get CS=0x23 (GDT[4]|RPL=3) and SS=0x1B (GDT[3]|RPL=3):
//!     STAR[63:48] + 16 = 0x20  →  STAR[63:48] = 0x10
//!     STAR[63:48] + 8  = 0x18  →  STAR[63:48] = 0x10  ✓
//!
//! NOTE: do NOT use 0x1B or 0x18 for STAR[63:48] — that shifts CS to GDT[5]
//! (the TSS descriptor), causing a #GP(0x28) on every sysretq.

use core::arch::asm;
use core::arch::naked_asm;
use crate::kernel::serial::SERIAL_PORT;
use super::syscall::handle_syscall;

const IA32_STAR:  u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const IA32_EFER:  u32 = 0xC000_0080;
const EFER_SCE:   u64 = 1 << 0;

// Dedicated kernel stack for syscall entry (16 KB).
// Must be a real static — the old hardcoded 0xFFFF800007E1F000 was unmapped.
static mut SYSCALL_STACK: [u8; 16 * 1024] = [0u8; 16 * 1024];

// Top-of-stack pointer, set at init() time.
static mut SYSCALL_STACK_TOP_ADDR: u64 = 0;

// Scratch slot to save the user RSP across the stack switch.
static mut USER_RSP_SAVE: u64 = 0;

pub unsafe fn init() {
    // Compute and store the real stack top address.
    SYSCALL_STACK_TOP_ADDR =
        core::ptr::addr_of!(SYSCALL_STACK) as u64 + (16 * 1024) as u64;

    // Enable SYSCALL/SYSRET in EFER.
    let mut efer = rdmsr(IA32_EFER);
    efer |= EFER_SCE;
    wrmsr(IA32_EFER, efer);

    // STAR[63:48] = 0x10 so that SYSRET gets:
    //   CS = (0x10 + 16) | RPL=3 = 0x23  (GDT[4] user-code)
    //   SS = (0x10 +  8) | RPL=3 = 0x1B  (GDT[3] user-data)
    // STAR[47:32] = 0x08  (kernel code selector for SYSCALL entry)
    let star: u64 = (0x10u64 << 48) | (0x08u64 << 32);
    wrmsr(IA32_STAR, star);

    // LSTAR: where syscall jumps.
    let entry_addr = syscall_entry as *const () as u64;
    wrmsr(IA32_LSTAR, entry_addr);

    // FMASK: clear IF (disable interrupts) on syscall entry.
    wrmsr(IA32_FMASK, 0x200);

    SERIAL_PORT.write_str("syscall/sysret enabled (STAR=0x10/0x08)\n");
}

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32; let hi: u32;
    asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi,
         options(nomem, nostack, preserves_flags));
    ((hi as u64) << 32) | lo as u64
}

#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    asm!("wrmsr", in("ecx") msr,
         in("eax") value as u32, in("edx") (value >> 32) as u32,
         options(nomem, nostack, preserves_flags));
}

// ── syscall entry trampoline ───────────────────────────────────────────────
//
// On entry (from `syscall` instruction):
//   rax = syscall number
//   rdi, rsi, rdx, r10, r8, r9 = args 1-6
//   rcx = user return RIP  (saved by syscall)
//   r11 = user RFLAGS     (saved by syscall)
//   rsp = still user RSP  (NOT switched by hardware)
//
// We must:
//   1. Save user RSP.
//   2. Switch to kernel stack.
//   3. Build a minimal stack frame, call handler.
//   4. Restore state, sysretq.
//
// sysretq uses rcx (return RIP) and r11 (RFLAGS) — we must preserve them.

// Stack layout after the 9 pushes below (rsp = lowest address):
//   [rsp + 0 ] = rax  (syscall number)
//   [rsp + 8 ] = rdi  (arg1)
//   [rsp + 16] = rsi  (arg2)
//   [rsp + 24] = rdx  (arg3)
//   [rsp + 32] = r8   (arg5, Linux syscall ABI)
//   [rsp + 40] = r9   (arg6, Linux syscall ABI)
//   [rsp + 48] = r10  (arg4, Linux uses r10 not rcx)
//   [rsp + 56] = rcx  (user RIP,    saved by syscall instruction)
//   [rsp + 64] = r11  (user RFLAGS, saved by syscall instruction)
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // Save user RSP; switch to mapped kernel stack.
        "mov [rip + {user_rsp}], rsp",
        "mov rsp, [rip + {stk_top}]",
        "and rsp, -16",

        // Push all registers we need to survive the call.
        "push r11",  // [rsp+64] user RFLAGS
        "push rcx",  // [rsp+56] user RIP
        "push r10",  // [rsp+48] arg4
        "push r9",   // [rsp+40] arg6
        "push r8",   // [rsp+32] arg5
        "push rdx",  // [rsp+24] arg3
        "push rsi",  // [rsp+16] arg2
        "push rdi",  // [rsp+ 8] arg1
        "push rax",  // [rsp+ 0] syscall number

        // Load args for handle_syscall(num, a1, a2, a3, a4, a5) [SysV AMD64].
        "mov rdi, [rsp]",       // num
        "mov rsi, [rsp +  8]",  // a1  = rdi
        "mov rdx, [rsp + 16]",  // a2  = rsi
        "mov rcx, [rsp + 24]",  // a3  = rdx
        "mov r8,  [rsp + 48]",  // a4  = r10
        "mov r9,  [rsp + 32]",  // a5  = r8

        "call {handler}",       // result in rax

        // Restore sysretq-required registers from saved slots.
        "mov rcx, [rsp + 56]",  // user RIP   → rcx
        "mov r11, [rsp + 64]",  // user RFLAGS → r11

        // Restore user stack, return to ring 3.
        "mov rsp, [rip + {user_rsp}]",
        "sysretq",

        user_rsp = sym USER_RSP_SAVE,
        stk_top  = sym SYSCALL_STACK_TOP_ADDR,
        handler  = sym syscall_handler_wrapper,
    );
}

#[unsafe(no_mangle)]
unsafe extern "C" fn syscall_handler_wrapper(
    syscall_num: u64,
    arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64,
) -> i64 {
    let result = handle_syscall(syscall_num, arg1, arg2, arg3, arg4, arg5);
    result.value
}
