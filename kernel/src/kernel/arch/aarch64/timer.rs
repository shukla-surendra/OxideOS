//! ARM generic timer (aarch64 counterpart of the PIT).
//!
//! Two distinct roles, deliberately kept separate:
//!
//!   * **Time source** — `get_ticks` derives ticks from the free-running
//!     `CNTPCT_EL0` counter, scaled to the same 100 Hz rate the x86 PIT driver
//!     reports, so shared code (GUI clock, uptime) works unchanged. This is
//!     monotonic and cannot drift.
//!   * **Tick event** — the EL1 physical timer raises INTID 30 at `TIMER_HZ`,
//!     which is what wakes `wfi` and (once B4 lands) will drive preemption.
//!
//! Counting handler invocations as the clock would accumulate error on every
//! late or coalesced interrupt, so the IRQ deliberately does *not* feed
//! `get_ticks` — it only signals that time has passed.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

/// Tick rate shared code assumes (x86 PIT is programmed to the same rate).
pub const TIMER_HZ: u64 = 100;

/// Timer IRQs taken since boot.  Distinct from `get_ticks()` — this counts
/// interrupts actually delivered, which is exactly what proves the GIC works.
static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);

fn counter() -> u64 {
    let cnt: u64;
    unsafe {
        asm!("isb", "mrs {}, cntpct_el0", out(reg) cnt, options(nomem, nostack, preserves_flags));
    }
    cnt
}

fn frequency() -> u64 {
    let frq: u64;
    unsafe {
        asm!("mrs {}, cntfrq_el0", out(reg) frq, options(nomem, nostack, preserves_flags));
    }
    frq
}

/// Ticks since boot at `TIMER_HZ`.
pub unsafe fn get_ticks() -> u64 {
    let frq = frequency();
    if frq == 0 {
        return 0;
    }
    // counter / (freq / HZ) keeps the intermediate math within u64 range.
    counter() / (frq / TIMER_HZ).max(1)
}

/// Silence the timer without needing the GIC.
///
/// UEFI (AAVMF) uses the generic timer during firmware execution and can hand
/// the kernel a timer that is still enabled with an interrupt already pending.
/// Unmasking `DAIF.I` in that state takes an immediate interrupt into a
/// handler that is not ready yet, so this runs before the vectors go in.
pub unsafe fn mask() {
    unsafe {
        asm!(
            "msr cntp_ctl_el0, xzr",
            "isb",
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Start the EL1 physical timer firing at `TIMER_HZ`.
///
/// Requires the GIC to be up and INTID 30 enabled; the caller unmasks
/// `DAIF.I` afterwards.
pub unsafe fn init_irq() {
    unsafe {
        arm_first_deadline();
        // CNTP_CTL_EL0: ENABLE (bit 0) set, IMASK (bit 1) clear.
        asm!(
            "mov {tmp}, #1",
            "msr cntp_ctl_el0, {tmp}",
            "isb",
            tmp = out(reg) _,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Counter ticks between two timer interrupts.
fn interval_ticks() -> u64 {
    let frq = frequency();
    if frq == 0 { 0 } else { (frq / TIMER_HZ).max(1) }
}

/// Absolute counter value the next tick is due at.
static NEXT_DEADLINE: AtomicU64 = AtomicU64::new(0);

unsafe fn set_deadline(at: u64) {
    NEXT_DEADLINE.store(at, Ordering::Relaxed);
    unsafe {
        asm!(
            "msr cntp_cval_el0, {}",
            "isb",
            in(reg) at,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Arm the comparator for one interval from now.
unsafe fn arm_first_deadline() {
    unsafe { set_deadline(counter() + interval_ticks()) };
}

/// Timer IRQ handler, called from `exceptions::aarch64_irq_handler`.
///
/// Advances an **absolute** deadline (`CNTP_CVAL_EL0`) by a fixed interval
/// rather than restarting a relative countdown (`CNTP_TVAL_EL0`). The
/// difference is not cosmetic: a relative re-arm starts counting when the
/// handler runs, so every period silently absorbs the interrupt latency and
/// the tick rate drifts slow — measured at ~20% under emulation during
/// bring-up (20 interrupts across 25 counter-derived ticks). With an absolute
/// deadline the interrupts stay locked to the counter no matter how late a
/// handler is.
///
/// Rewriting the comparator is also what deasserts the interrupt — the timer
/// condition holds while `CNTPCT >= CNTP_CVAL`, so skipping this would
/// re-enter the handler immediately after EOI.
pub unsafe fn handle_irq() {
    IRQ_COUNT.fetch_add(1, Ordering::Relaxed);

    let interval = interval_ticks();
    let mut next = NEXT_DEADLINE.load(Ordering::Relaxed).wrapping_add(interval);
    let now = counter();
    // If we fell more than a whole period behind (a long handler, or a halted
    // guest under a debugger), resync instead of replaying a backlog of
    // deadlines that would fire back-to-back.
    if next <= now {
        next = now + interval;
    }
    unsafe { set_deadline(next) };
}

/// Timer interrupts delivered since boot.
pub fn irq_count() -> u64 {
    IRQ_COUNT.load(Ordering::Relaxed)
}
