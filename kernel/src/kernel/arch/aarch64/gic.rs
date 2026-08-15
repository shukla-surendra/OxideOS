//! GICv2 interrupt controller — QEMU `virt` board (aarch64 counterpart of the
//! x86 8259 PIC in `drivers/pic.rs`).
//!
//! The GIC has two register blocks with different jobs:
//!
//!   * **Distributor** (`GICD`, 0x0800_0000) — system-wide: which INTIDs exist,
//!     their priority, whether they are enabled, and which CPU they target.
//!   * **CPU interface** (`GICC`, 0x0801_0000) — per-core: acknowledge the
//!     interrupt currently being signalled, and tell the GIC when it is done.
//!
//! INTID space (GICv2):
//!
//! ```text
//!   0..15    SGI   software-generated (inter-processor)
//!  16..31    PPI   private peripheral — per-CPU, banked (the timer lives here)
//!  32..      SPI   shared peripheral — routed to a CPU via GICD_ITARGETSR
//! ```
//!
//! Like `serial.rs`, register access starts at the physical address and
//! re-bases onto the Limine HHDM in `init()`.

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// GIC distributor on the QEMU `virt` machine.
pub const GICD_PHYS: u64 = 0x0800_0000;
/// GIC CPU interface on the QEMU `virt` machine.
pub const GICC_PHYS: u64 = 0x0801_0000;

// ── Distributor register offsets ────────────────────────────────────────────
const GICD_CTLR: u64 = 0x000; // control
const GICD_TYPER: u64 = 0x004; // how many INTIDs this GIC implements
const GICD_IGROUPR: u64 = 0x080; // 1 bit per INTID  — security group
const GICD_ISENABLER: u64 = 0x100; // 1 bit per INTID  — set-enable
const GICD_ICENABLER: u64 = 0x180; // 1 bit per INTID  — clear-enable
const GICD_ICPENDR: u64 = 0x280; // 1 bit per INTID  — clear-pending
const GICD_IPRIORITYR: u64 = 0x400; // 1 byte per INTID — priority
const GICD_ITARGETSR: u64 = 0x800; // 1 byte per INTID — CPU target mask

// ── CPU interface register offsets ──────────────────────────────────────────
const GICC_CTLR: u64 = 0x00; // control
const GICC_PMR: u64 = 0x04; // priority mask
const GICC_BPR: u64 = 0x08; // binary point (preemption grouping)
const GICC_IAR: u64 = 0x0C; // interrupt acknowledge
const GICC_EOIR: u64 = 0x10; // end of interrupt

/// Returned by [`acknowledge`] when there is nothing to service.  Must **not**
/// be EOI'd — writing it back to `GICC_EOIR` is a protocol violation.
pub const INTID_SPURIOUS: u32 = 1023;

/// EL1 physical timer — PPI 14, so INTID 16 + 14.
pub const INTID_TIMER: u32 = 30;
/// PL011 UART0 — SPI 1, so INTID 32 + 1.
pub const INTID_UART: u32 = 33;
/// virtio-mmio slot 0 — SPI 16, so INTID 32 + 16.  Slot *n* is this + *n*.
pub const INTID_VIRTIO_MMIO_BASE: u32 = 48;

/// Priority given to every INTID.  Lower value = higher priority on a GIC;
/// one shared mid-range level means no interrupt can preempt another, which
/// is what a kernel with a single non-reentrant IRQ path wants.
const DEFAULT_PRIORITY: u8 = 0xA0;

static GICD_BASE: AtomicU64 = AtomicU64::new(GICD_PHYS);
static GICC_BASE: AtomicU64 = AtomicU64::new(GICC_PHYS);
/// Number of INTIDs this GIC implements, learned from `GICD_TYPER` in `init`.
static NUM_INTIDS: AtomicU32 = AtomicU32::new(0);

fn gicd(offset: u64) -> *mut u32 {
    (GICD_BASE.load(Ordering::Relaxed) + offset) as *mut u32
}

fn gicc(offset: u64) -> *mut u32 {
    (GICC_BASE.load(Ordering::Relaxed) + offset) as *mut u32
}

/// Bring the distributor and this CPU's interface up, with every INTID
/// disabled.  Callers enable the ones they want via [`enable`].
///
/// Interrupts must still be masked at the CPU (`DAIF.I`) when this runs —
/// `cpu::irq_enable()` is the caller's job, once handlers are installed.
pub unsafe fn init() {
    if let Some(resp) = crate::HHDM_REQUEST.get_response() {
        GICD_BASE.store(resp.offset() + GICD_PHYS, Ordering::Relaxed);
        GICC_BASE.store(resp.offset() + GICC_PHYS, Ordering::Relaxed);
    }

    unsafe {
        // Quiesce the distributor before touching anything else — UEFI ran
        // before us and may have left interrupts enabled and pending.
        gicd(GICD_CTLR).write_volatile(0);

        // GICD_TYPER.ITLinesNumber (bits 4:0): the GIC implements
        // 32 * (ITLinesNumber + 1) INTIDs, capped at 1020 by the spec.
        let it_lines = gicd(GICD_TYPER).read_volatile() & 0x1F;
        let num_intids = core::cmp::min(32 * (it_lines + 1), 1020);
        NUM_INTIDS.store(num_intids, Ordering::Relaxed);

        // Per 32-INTID block: disable everything, drop stale pending state,
        // and put every interrupt in group 0.  (On a GICv2 built without the
        // security extensions — QEMU `virt` with no EL3 — IGROUPR reads as
        // zero and ignores writes; writing 0 is correct either way.)
        for block in 0..(num_intids / 32) {
            let off = (block * 4) as u64;
            gicd(GICD_ICENABLER + off).write_volatile(0xFFFF_FFFF);
            gicd(GICD_ICPENDR + off).write_volatile(0xFFFF_FFFF);
            gicd(GICD_IGROUPR + off).write_volatile(0);
        }

        // One priority for all, written a byte at a time.
        for intid in 0..num_intids {
            gicd(GICD_IPRIORITYR + intid as u64)
                .cast::<u8>()
                .write_volatile(DEFAULT_PRIORITY);
        }

        // Route every SPI to CPU 0.  INTIDs below 32 are SGIs/PPIs — banked
        // per-CPU, with a read-only ITARGETSR — so they are skipped.
        for intid in 32..num_intids {
            gicd(GICD_ITARGETSR + intid as u64)
                .cast::<u8>()
                .write_volatile(0x01);
        }

        gicd(GICD_CTLR).write_volatile(1); // distributor on

        // CPU interface.  PMR first and deliberately: it resets to 0, which
        // masks *every* priority — leaving it there is the classic "the GIC
        // is fully configured and no interrupt ever fires" bug.  0xFF admits
        // all priorities.
        gicc(GICC_PMR).write_volatile(0xFF);
        gicc(GICC_BPR).write_volatile(0x07); // no preemption grouping
        gicc(GICC_CTLR).write_volatile(1); // CPU interface on

        // Make every write above visible before any caller unmasks DAIF.I.
        asm!("dsb sy", "isb", options(nostack, preserves_flags));
    }
}

/// Let `intid` reach this CPU.
pub unsafe fn enable(intid: u32) {
    unsafe {
        let off = ((intid / 32) * 4) as u64;
        gicd(GICD_ISENABLER + off).write_volatile(1 << (intid % 32));
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Stop `intid` reaching this CPU.
pub unsafe fn disable(intid: u32) {
    unsafe {
        let off = ((intid / 32) * 4) as u64;
        gicd(GICD_ICENABLER + off).write_volatile(1 << (intid % 32));
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Claim the interrupt being signalled.  Returns the raw `GICC_IAR` value —
/// pass it back to [`end_of_interrupt`] **unmodified**, and use [`intid_of`]
/// to get the interrupt number out of it.
///
/// For SGIs the upper bits carry the sending CPU, which the GIC needs back on
/// EOI, so the raw value (not just the INTID) is what must be returned.
pub unsafe fn acknowledge() -> u32 {
    unsafe { gicc(GICC_IAR).read_volatile() }
}

/// The interrupt number carried in a `GICC_IAR` value (bits 9:0).
pub fn intid_of(iar: u32) -> u32 {
    iar & 0x3FF
}

/// Report the interrupt claimed by [`acknowledge`] as handled.
pub unsafe fn end_of_interrupt(iar: u32) {
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
        gicc(GICC_EOIR).write_volatile(iar);
    }
}

/// Number of INTIDs the distributor reports, or 0 before [`init`].
pub fn num_intids() -> u32 {
    NUM_INTIDS.load(Ordering::Relaxed)
}
