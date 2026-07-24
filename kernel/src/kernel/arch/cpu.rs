//! Portable CPU-control primitives.
//!
//! Every operation the portable kernel needs from the CPU — masking
//! interrupts, halting, spin-loop hints — lives here with one implementation
//! per architecture.  Shared code (GUI, scheduler glue, boot sequencing,
//! panic handler) must call these instead of writing `asm!` directly; raw
//! assembly is allowed only inside `arch/<target>/` and arch-specific
//! drivers.
//!
//! x86-64                          aarch64
//! ──────────────────────────────  ─────────────────────────────────────────
//! `cli` / `sti`                   `msr daifset/daifclr, #2` (mask/unmask IRQ)
//! `pushfq` → RFLAGS.IF            `mrs DAIF` → I bit (note: inverted sense)
//! `hlt`                           `wfi` (wait for interrupt)
//! `pause`                         `yield`

use core::arch::asm;

/// Disable (mask) interrupts on the current CPU.
#[inline(always)]
pub fn irq_disable() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("cli", options(nostack, nomem, preserves_flags));
        #[cfg(target_arch = "aarch64")]
        asm!("msr daifset, #2", options(nostack, nomem, preserves_flags));
    }
}

/// Enable (unmask) interrupts on the current CPU.
#[inline(always)]
pub fn irq_enable() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("sti", options(nostack, nomem, preserves_flags));
        #[cfg(target_arch = "aarch64")]
        asm!("msr daifclr, #2", options(nostack, nomem, preserves_flags));
    }
}

/// True if interrupts are currently enabled on this CPU.
#[inline(always)]
pub fn irqs_enabled() -> bool {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        {
            let flags: u64;
            asm!("pushfq; pop {}", out(reg) flags, options(nomem, preserves_flags));
            flags & (1 << 9) != 0 // RFLAGS.IF
        }
        #[cfg(target_arch = "aarch64")]
        {
            let daif: u64;
            asm!("mrs {}, daif", out(reg) daif, options(nostack, nomem, preserves_flags));
            daif & (1 << 7) == 0 // DAIF.I set means IRQs *masked*
        }
    }
}

/// Disable interrupts and return the previous "enabled" state for
/// [`irq_restore`].  Use around short critical sections.
#[inline(always)]
pub fn irq_save() -> bool {
    let was_enabled = irqs_enabled();
    irq_disable();
    was_enabled
}

/// Restore the interrupt state captured by [`irq_save`].
#[inline(always)]
pub fn irq_restore(was_enabled: bool) {
    if was_enabled {
        irq_enable();
    }
}

/// Sleep the CPU until the next interrupt (x86) or wake event (aarch64).
///
/// aarch64 uses `wfe`, not `wfi`: until the GIC is programmed no interrupt
/// can ever become pending, so `wfi` would sleep forever.  The generic-timer
/// event stream (see `arch::aarch64::timer::enable_event_stream`) wakes `wfe`
/// every ~0.5 ms instead.  Switches to `wfi` once the GIC + timer IRQ land.
#[inline(always)]
pub fn wait_for_interrupt() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("hlt", options(nostack, nomem, preserves_flags));
        #[cfg(target_arch = "aarch64")]
        asm!("wfe", options(nostack, nomem, preserves_flags));
    }
}

/// Politeness hint inside a busy-wait loop (x86 `pause`, ARM `yield`).
#[inline(always)]
pub fn spin_hint() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("pause", options(nostack, nomem, preserves_flags));
        #[cfg(target_arch = "aarch64")]
        asm!("yield", options(nostack, nomem, preserves_flags));
    }
}

/// Mask interrupts and halt forever.  Terminal state for panics and
/// unrecoverable errors.
pub fn halt_forever() -> ! {
    irq_disable();
    loop {
        wait_for_interrupt();
    }
}
