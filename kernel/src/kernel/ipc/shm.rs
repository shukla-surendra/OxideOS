//! Shared memory (shmget / shmat / shmdt) for OxideOS.
//!
//! # Design
//!
//! Up to `MAX_SEGMENTS` named segments live in a global table, each backed by
//! physically-allocated frames.  When a process calls `shmat`, those SAME
//! physical frames are mapped into the caller's address space, giving true
//! zero-copy sharing between processes.
//!
//! # Address layout (per process, per segment)
//!
//! ```
//! SHM_VIRT_BASE + shmid * SHM_SLOT_SIZE
//! ```
//! Default: `0x2000_0000 + id × 1 MB`.  The region is above `USER_MMAP_BASE`
//! (0x0800_0000) and the user heap, so it does not collide with brk or mmap.

use crate::kernel::paging_allocator as pa;

// ── Limits ────────────────────────────────────────────────────────────────────

/// Maximum number of shared memory segments.
pub const MAX_SEGMENTS: usize = 16;
/// Maximum size of a single segment (1 MB = 256 pages).
pub const MAX_SEG_SIZE: u64 = 0x0010_0000;
/// Maximum simultaneous attachments per process.
pub const MAX_ATTACH:   usize = 8;

const PAGE_SIZE: u64 = 4096;

/// Virtual base in each process where shm segments are mapped.
const SHM_VIRT_BASE: u64 = 0x2000_0000;
/// Each slot occupies 1 MB of virtual address space.
const SHM_SLOT_SIZE: u64 = 0x0010_0000;

// ── Segment table ─────────────────────────────────────────────────────────────

struct ShmSegment {
    key:       u32,
    size:      u64,   // actual size in bytes (multiple of PAGE_SIZE)
    phys_base: u64,   // physical address of frame 0
    pages:     usize,
    refcount:  u32,
    active:    bool,
}

impl ShmSegment {
    const fn empty() -> Self {
        Self { key: 0, size: 0, phys_base: 0, pages: 0, refcount: 0, active: false }
    }
}

static mut SEGTAB: [ShmSegment; MAX_SEGMENTS] = [const { ShmSegment::empty() }; MAX_SEGMENTS];

// ── Per-process attachment record ─────────────────────────────────────────────

/// One entry in a task's `shm_attaches` array.
#[derive(Clone, Copy)]
pub struct ShmAttach {
    pub shmid:  u32,
    pub vaddr:  u64,
    pub active: bool,
}

impl ShmAttach {
    pub const fn empty() -> Self {
        Self { shmid: 0, vaddr: 0, active: false }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Create or open a shared memory segment.
///
/// Returns the segment id (≥ 0) or a negative error code.
pub unsafe fn shmget(key: u32, size: u64, _flags: u32) -> i64 {
    if size == 0 || size > MAX_SEG_SIZE { return -22; } // EINVAL

    let segtab = &raw mut SEGTAB;

    // Return existing segment if the key matches.
    for i in 0..MAX_SEGMENTS {
        if (*segtab)[i].active && (*segtab)[i].key == key {
            return i as i64;
        }
    }

    // Allocate a new slot.
    let slot = match (0..MAX_SEGMENTS).find(|&i| !(*segtab)[i].active) {
        Some(s) => s,
        None    => return -28, // ENOSPC
    };

    let pages  = ((size + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    let actual = pages as u64 * PAGE_SIZE;

    // Allocate physical frames (zeroed).
    let phys = unsafe { pa::alloc_phys_frames(pages) };
    if phys == 0 { return -12; } // ENOMEM

    (*segtab)[slot] = ShmSegment {
        key, size: actual, phys_base: phys, pages, refcount: 0, active: true,
    };
    slot as i64
}

/// Map segment `shmid` into the calling process.
///
/// `attaches` is the calling task's attachment table; `cr3` is its page table.
/// Returns the virtual address (as a positive i64) or a negative error.
pub unsafe fn shmat(
    shmid:   u32,
    attaches: &mut [ShmAttach; MAX_ATTACH],
    cr3:     u64,
) -> i64 {
    let segtab = &raw mut SEGTAB;
    let id     = shmid as usize;

    if id >= MAX_SEGMENTS || !(*segtab)[id].active { return -22; } // EINVAL

    // Already attached?
    for a in attaches.iter() {
        if a.active && a.shmid == shmid { return a.vaddr as i64; }
    }

    // Find a free attachment slot.
    let slot = match attaches.iter().position(|a| !a.active) {
        Some(s) => s,
        None    => return -24, // EMFILE
    };

    let vaddr = SHM_VIRT_BASE + id as u64 * SHM_SLOT_SIZE;
    let pages = (*segtab)[id].pages;
    let phys  = (*segtab)[id].phys_base;

    // Map the physical frames into this process's address space.
    if unsafe { pa::map_phys_pages_in(cr3, vaddr, phys, pages, true) }.is_err() {
        return -12; // ENOMEM
    }

    (*segtab)[id].refcount += 1;
    attaches[slot] = ShmAttach { shmid, vaddr, active: true };
    vaddr as i64
}

/// Return the physical base address of segment `shmid`, or 0 if not valid.
/// Used by the compositor to access shm data via the HHDM.
pub fn seg_phys_base(shmid: usize) -> u64 {
    if shmid >= MAX_SEGMENTS { return 0; }
    unsafe {
        let segtab = &raw const SEGTAB;
        if (*segtab)[shmid].active { (*segtab)[shmid].phys_base } else { 0 }
    }
}

/// Detach the shared segment previously attached at `addr`.
pub unsafe fn shmdt(addr: u64, attaches: &mut [ShmAttach; MAX_ATTACH]) -> i64 {
    let segtab = &raw mut SEGTAB;

    let slot = match attaches.iter().position(|a| a.active && a.vaddr == addr) {
        Some(s) => s,
        None    => return -22,
    };

    let shmid = attaches[slot].shmid as usize;
    attaches[slot] = ShmAttach::empty();

    if shmid < MAX_SEGMENTS && (*segtab)[shmid].active {
        (*segtab)[shmid].refcount = (*segtab)[shmid].refcount.saturating_sub(1);
        // Segments live for the OS lifetime; we don't free on last detach.
    }
    0
}
