//! ARM generic timer as a polled tick source (aarch64 counterpart of the PIT).
//!
//! No GIC/IRQs yet — `get_ticks` derives ticks directly from the free-running
//! CNTPCT_EL0 counter, scaled to the same 100 Hz rate the x86 PIT driver
//! reports, so shared code (GUI clock, uptime) works unchanged.

use core::arch::asm;

/// Tick rate shared code assumes (x86 PIT is programmed to the same rate).
pub const TIMER_HZ: u64 = 100;

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

/// Enable the counter event stream: wakes `wfe` on every transition of
/// counter bit 14 (~0.5 ms at QEMU's 62.5 MHz), so the GUI loop's
/// `wait_for_interrupt` gets a heartbeat before the GIC exists.
pub unsafe fn enable_event_stream() {
    const EVNTEN: u64 = 1 << 2;
    const EVNTI_BIT14: u64 = 14 << 4;
    unsafe {
        asm!(
            "msr cntkctl_el1, {}",
            "isb",
            in(reg) EVNTEN | EVNTI_BIT14,
            options(nomem, nostack, preserves_flags),
        );
    }
}
