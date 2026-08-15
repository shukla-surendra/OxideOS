//! EL1 exception vector table (aarch64 counterpart of the x86 IDT).
//!
//! Sixteen 128-byte slots, 2 KiB aligned: four vector groups (current EL with
//! SP_EL0, current EL with SP_ELx, lower EL AArch64, lower EL AArch32), each
//! with Synchronous / IRQ / FIQ / SError entries.
//!
//! Bring-up policy: every exception funnels into one Rust handler that dumps
//! ESR/ELR/FAR to the PL011 and parks the CPU.  IRQ dispatch arrives with the
//! GIC in the next port step.

use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, Ordering};

use super::gic;
use super::serial::SERIAL_PORT;
use crate::kernel::arch::cpu;

global_asm!(
    r#"
    .macro OXIDE_VECTOR idx
    .balign 0x80
        mov     x0, #\idx
        b       aarch64_exception_entry
    .endm

    // IRQ slots differ from every other vector: they must *return*.
    .macro OXIDE_IRQ_VECTOR
    .balign 0x80
        b       aarch64_irq_entry
    .endm

    .section .text
    .balign 0x800
    .global aarch64_vector_table
aarch64_vector_table:
    OXIDE_VECTOR 0      // Current EL, SP_EL0: Synchronous
    OXIDE_IRQ_VECTOR    // Current EL, SP_EL0: IRQ  ← the live one (Limine
                        //   enters with SPSel=0, so this is the slot that fires)
    OXIDE_VECTOR 2      // Current EL, SP_EL0: FIQ
    OXIDE_VECTOR 3      // Current EL, SP_EL0: SError
    OXIDE_VECTOR 4      // Current EL, SP_ELx: Synchronous
    OXIDE_IRQ_VECTOR    // Current EL, SP_ELx: IRQ  (once anything runs on SP_EL1)
    OXIDE_VECTOR 6      // Current EL, SP_ELx: FIQ
    OXIDE_VECTOR 7      // Current EL, SP_ELx: SError
    OXIDE_VECTOR 8      // Lower EL, AArch64: Synchronous
    OXIDE_VECTOR 9      // Lower EL, AArch64: IRQ
    OXIDE_VECTOR 10     // Lower EL, AArch64: FIQ
    OXIDE_VECTOR 11     // Lower EL, AArch64: SError
    OXIDE_VECTOR 12     // Lower EL, AArch32: Synchronous
    OXIDE_VECTOR 13     // Lower EL, AArch32: IRQ
    OXIDE_VECTOR 14     // Lower EL, AArch32: FIQ
    OXIDE_VECTOR 15     // Lower EL, AArch32: SError

    // Handlers never return during bring-up, so no frame save/restore yet.
aarch64_exception_entry:
        mrs     x1, esr_el1
        mrs     x2, elr_el1
        mrs     x3, far_el1
        b       aarch64_exception_handler

    // IRQ path — the one exception entry that returns, so it must leave the
    // interrupted context byte-identical.  The Rust handler follows AAPCS64
    // and preserves x19-x28 itself, so only the caller-saved set needs saving:
    // x0-x18 plus the frame pointer and link register.  22 slots keeps SP
    // 16-byte aligned.
    //
    // ELR_EL1/SPSR_EL1 are deliberately not saved: `eret` consumes the values
    // the CPU wrote on entry, and DAIF.I stays masked for the whole handler,
    // so nothing can nest and overwrite them.  B4 (scheduler context switch)
    // is what turns this into a full trap frame.
aarch64_irq_entry:
        sub     sp, sp, #176
        stp     x0,  x1,  [sp, #0]
        stp     x2,  x3,  [sp, #16]
        stp     x4,  x5,  [sp, #32]
        stp     x6,  x7,  [sp, #48]
        stp     x8,  x9,  [sp, #64]
        stp     x10, x11, [sp, #80]
        stp     x12, x13, [sp, #96]
        stp     x14, x15, [sp, #112]
        stp     x16, x17, [sp, #128]
        stp     x18, x29, [sp, #144]
        str     x30,      [sp, #160]

        bl      aarch64_irq_handler

        ldp     x0,  x1,  [sp, #0]
        ldp     x2,  x3,  [sp, #16]
        ldp     x4,  x5,  [sp, #32]
        ldp     x6,  x7,  [sp, #48]
        ldp     x8,  x9,  [sp, #64]
        ldp     x10, x11, [sp, #80]
        ldp     x12, x13, [sp, #96]
        ldp     x14, x15, [sp, #112]
        ldp     x16, x17, [sp, #128]
        ldp     x18, x29, [sp, #144]
        ldr     x30,      [sp, #160]
        add     sp, sp, #176
        eret

    // Limine enters the kernel with SPSel=0 (running on SP_EL0), so exceptions
    // taken through the SP_EL0 vectors pivot onto SP_EL1 — which the bootloader
    // leaves uninitialised.  Point SP_EL1 at a dedicated exception stack so the
    // handler prologue doesn't itself fault (observed as a nested data abort
    // at the first `stp` during bring-up).
    .section .bss
    .balign 16
oxide_exception_stack:
    .skip 0x4000
oxide_exception_stack_top:

    .section .text
    .global aarch64_install_vectors
aarch64_install_vectors:
        adrp    x0, aarch64_vector_table
        add     x0, x0, :lo12:aarch64_vector_table
        msr     vbar_el1, x0
        adrp    x1, oxide_exception_stack_top
        add     x1, x1, :lo12:oxide_exception_stack_top
        mrs     x2, spsel
        cbnz    x2, 1f          // already on SP_EL1 — don't clobber the live stack
        msr     spsel, #1
        mov     sp, x1
        msr     spsel, #0
1:
        isb
        ret
    "#
);

unsafe extern "C" {
    fn aarch64_install_vectors();
}

/// Point VBAR_EL1 at our vector table.
pub unsafe fn init() {
    unsafe { aarch64_install_vectors() };
}

/// Exception level we are currently running at (Limine enters us at EL1).
pub fn current_el() -> u64 {
    let el: u64;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) el, options(nomem, nostack, preserves_flags));
    }
    (el >> 2) & 0b11
}

const VECTOR_NAMES: [&str; 16] = [
    "Sync (EL1, SP_EL0)", "IRQ (EL1, SP_EL0)", "FIQ (EL1, SP_EL0)", "SError (EL1, SP_EL0)",
    "Sync (EL1, SP_EL1)", "IRQ (EL1, SP_EL1)", "FIQ (EL1, SP_EL1)", "SError (EL1, SP_EL1)",
    "Sync (EL0, A64)",    "IRQ (EL0, A64)",    "FIQ (EL0, A64)",    "SError (EL0, A64)",
    "Sync (EL0, A32)",    "IRQ (EL0, A32)",    "FIQ (EL0, A32)",    "SError (EL0, A32)",
];

/// Full-width 16-digit hex, e.g. `0x00000DEADBEEF000`.
pub unsafe fn write_hex64(value: u64) {
    unsafe {
        SERIAL_PORT.write_str("0x");
        for shift in (0..16).rev() {
            let digit = ((value >> (shift * 4)) & 0xF) as u8;
            SERIAL_PORT.write_byte(if digit < 10 { b'0' + digit } else { b'A' + (digit - 10) });
        }
    }
}

/// Interrupts seen since boot that no handler claimed, by INTID.  A rising
/// count here means something was enabled in the GIC without being wired up.
static SPURIOUS_COUNT: AtomicU64 = AtomicU64::new(0);
static UNHANDLED_COUNT: AtomicU64 = AtomicU64::new(0);

/// Called from `aarch64_irq_entry` with interrupts masked.
///
/// Claim → dispatch → EOI is the GIC's required order; skipping the EOI leaves
/// the interrupt at "active" forever and the CPU never sees another one at the
/// same or lower priority, which presents as "the first tick works and then
/// everything stops".
#[unsafe(no_mangle)]
extern "C" fn aarch64_irq_handler() {
    unsafe {
        let iar = gic::acknowledge();
        let intid = gic::intid_of(iar);

        // The spurious INTID means the interrupt was withdrawn before we
        // claimed it.  It is the one value that must not be written to EOIR.
        if intid == gic::INTID_SPURIOUS {
            SPURIOUS_COUNT.fetch_add(1, Ordering::Relaxed);
            return;
        }

        match intid {
            gic::INTID_TIMER => super::timer::handle_irq(),
            _ => {
                UNHANDLED_COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }

        gic::end_of_interrupt(iar);
    }
}

/// (spurious, unhandled) IRQ counts since boot — boot-time diagnostics.
pub fn irq_anomaly_counts() -> (u64, u64) {
    (
        SPURIOUS_COUNT.load(Ordering::Relaxed),
        UNHANDLED_COUNT.load(Ordering::Relaxed),
    )
}

#[unsafe(no_mangle)]
extern "C" fn aarch64_exception_handler(vector: u64, esr: u64, elr: u64, far: u64) -> ! {
    unsafe {
        SERIAL_PORT.write_str("\n!!! EL1 EXCEPTION: ");
        SERIAL_PORT.write_str(VECTOR_NAMES[(vector & 0xF) as usize]);
        SERIAL_PORT.write_str("\n  ESR_EL1: ");
        write_hex64(esr);
        SERIAL_PORT.write_str("  (EC=");
        SERIAL_PORT.write_hex(((esr >> 26) & 0x3F) as u32);
        SERIAL_PORT.write_str(")\n  ELR_EL1: ");
        write_hex64(elr);
        SERIAL_PORT.write_str("\n  FAR_EL1: ");
        write_hex64(far);
        SERIAL_PORT.write_str("\nCPU parked.\n");
    }
    cpu::halt_forever()
}
