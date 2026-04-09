// src/kernel/paging_allocator.rs
//! Page Table Based Memory Allocator for OxideOS
//!
//! Allocates virtual memory by mapping physical frames on demand.
//! Deallocation unmaps pages, frees physical frames, and recycles
//! virtual address ranges via a fixed-size free list so the virtual
//! address space is not exhausted on long-running workloads.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use core::cell::UnsafeCell;
use limine::memory_map::EntryType;
use limine::request::MemoryMapRequest;
use crate::kernel::serial::SERIAL_PORT;

// ============================================================================
// PAGE TABLE STRUCTURES (x86_64)
// ============================================================================

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
struct PageTableFlags(u64);

impl PageTableFlags {
    const PRESENT:      u64 = 1 << 0;
    const WRITABLE:     u64 = 1 << 1;
    const USER:         u64 = 1 << 2;
    const NO_EXECUTE:   u64 = 1 << 63;

    fn kernel_flags() -> Self { Self(Self::PRESENT | Self::WRITABLE) }

    fn user_flags(writable: bool, executable: bool) -> Self {
        let mut f = Self(Self::PRESENT | Self::USER);
        if writable    { f.0 |= Self::WRITABLE; }
        if !executable { f.0 |= Self::NO_EXECUTE; }
        f
    }

    fn parent_table_flags(&self) -> Self {
        let mut f = Self(Self::PRESENT | Self::WRITABLE);
        if self.0 & Self::USER != 0 { f.0 |= Self::USER; }
        f
    }

    fn merge(&mut self, other: Self) { self.0 |= other.0; }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
struct PageTableEntry(u64);

impl PageTableEntry {
    fn new() -> Self { Self(0) }
    fn is_present(&self) -> bool { self.0 & PageTableFlags::PRESENT != 0 }
    fn flags(&self) -> PageTableFlags { PageTableFlags(self.0 & 0xFFF) }
    fn addr(&self)  -> u64 { self.0 & 0x000F_FFFF_FFFF_F000 }
    fn set(&mut self, addr: u64, flags: PageTableFlags) {
        self.0 = (addr & 0x000F_FFFF_FFFF_F000) | (flags.0 & 0xFFF);
    }
    fn clear(&mut self) { self.0 = 0; }
}

#[repr(align(4096))]
struct PageTable { entries: [PageTableEntry; 512] }

impl PageTable {
    fn new()  -> Self { Self { entries: [PageTableEntry::new(); 512] } }
    fn zero(&mut self) { for e in &mut self.entries { e.clear(); } }
}

// ============================================================================
// PHYSICAL FRAME ALLOCATOR  (bitmap, 256 MB range)
// ============================================================================

struct PhysicalFrameAllocator {
    bitmap:           [u64; 1024],   // 1024×64 = 65536 frames = 256 MB
    next_frame:       AtomicUsize,
    total_frames:     usize,
    allocated_frames: AtomicUsize,
}

impl PhysicalFrameAllocator {
    const fn new() -> Self {
        Self {
            bitmap:           [0; 1024],
            next_frame:       AtomicUsize::new(0),
            total_frames:     0,
            allocated_frames: AtomicUsize::new(0),
        }
    }

    unsafe fn init(&mut self, memory_map: &MemoryMapRequest) {
        unsafe { SERIAL_PORT.write_str("=== INITIALIZING PHYSICAL FRAME ALLOCATOR ===\n") };

        if let Some(map) = memory_map.get_response() {
            for word in &mut self.bitmap { *word = u64::MAX; } // all used

            for entry in map.entries() {
                if entry.entry_type == EntryType::USABLE {
                    let start = (entry.base   as usize) / 4096;
                    let count = (entry.length as usize) / 4096;
                    let safe  = core::cmp::max(start, 4096); // skip first 16 MB

                    if safe < 65536 {
                        let end = core::cmp::min(start + count, 65536);
                        for frame in safe..end {
                            self.mark_free(frame);
                            self.total_frames += 1;
                        }
                        unsafe {
                            SERIAL_PORT.write_str("  Tracked frames ");
                            SERIAL_PORT.write_decimal(safe as u32);
                            SERIAL_PORT.write_str(" - ");
                            SERIAL_PORT.write_decimal(end as u32);
                            SERIAL_PORT.write_str("\n");
                        }
                    }
                }
            }

            unsafe {
                SERIAL_PORT.write_str("Total trackable frames: ");
                SERIAL_PORT.write_decimal(self.total_frames as u32);
                SERIAL_PORT.write_str(" (");
                SERIAL_PORT.write_decimal((self.total_frames * 4) as u32);
                SERIAL_PORT.write_str(" KB)\n");
            }
        }
    }

    fn mark_free(&mut self, frame: usize) {
        if frame < 65536 { self.bitmap[frame / 64] &= !(1u64 << (frame % 64)); }
    }
    fn mark_used(&mut self, frame: usize) {
        if frame < 65536 { self.bitmap[frame / 64] |=   1u64 << (frame % 64);  }
    }
    fn is_free(&self, frame: usize) -> bool {
        frame < 65536 && (self.bitmap[frame / 64] & (1u64 << (frame % 64))) == 0
    }

    fn allocate_frame(&mut self) -> Option<u64> {
        let start = self.next_frame.load(Ordering::Relaxed);
        for off in 0..self.total_frames {
            let frame = (start + off) % 65536;
            if self.is_free(frame) {
                self.mark_used(frame);
                self.next_frame.store((frame + 1) % 65536, Ordering::Relaxed);
                self.allocated_frames.fetch_add(1, Ordering::Relaxed);
                return Some((frame * 4096) as u64);
            }
        }
        None
    }

    fn free_frame(&mut self, addr: u64) {
        let frame = (addr / 4096) as usize;
        if frame < 65536 {
            self.mark_free(frame);
            self.allocated_frames.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

// ============================================================================
// PAGE TABLE MANAGER
// ============================================================================

struct PageTableManager {
    l4_table_phys:      u64,
    higher_half_offset: u64,
}

impl PageTableManager {
    fn new(higher_half_offset: u64) -> Self {
        let cr3: u64;
        unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3); }
        Self { l4_table_phys: cr3 & 0x000F_FFFF_FFFF_F000, higher_half_offset }
    }

    fn phys_to_virt(&self, phys: u64) -> *mut u8 {
        (phys + self.higher_half_offset) as *mut u8
    }

    unsafe fn get_table(&self, phys: u64) -> &mut PageTable {
        unsafe { &mut *(self.phys_to_virt(phys) as *mut PageTable) }
    }

    /// Walk/allocate the four-level page table and install a leaf mapping.
    unsafe fn map(
        &mut self,
        virt:        u64,
        phys:        u64,
        flags:       PageTableFlags,
        frame_alloc: &mut PhysicalFrameAllocator,
    ) -> Result<(), &'static str> {
        let pf = flags.parent_table_flags();
        let l4i = ((virt >> 39) & 0x1FF) as usize;
        let l3i = ((virt >> 30) & 0x1FF) as usize;
        let l2i = ((virt >> 21) & 0x1FF) as usize;
        let l1i = ((virt >> 12) & 0x1FF) as usize;

        unsafe {
            // L4 → L3
            let l4 = self.get_table(self.l4_table_phys);
            let l3_phys = if l4.entries[l4i].is_present() {
                let mut f = l4.entries[l4i].flags(); f.merge(pf);
                l4.entries[l4i].set(l4.entries[l4i].addr(), f);
                l4.entries[l4i].addr()
            } else {
                let t = frame_alloc.allocate_frame().ok_or("OOM: L3 table")?;
                l4.entries[l4i].set(t, pf);
                self.get_table(t).zero(); t
            };

            // L3 → L2
            let l3 = self.get_table(l3_phys);
            let l2_phys = if l3.entries[l3i].is_present() {
                let mut f = l3.entries[l3i].flags(); f.merge(pf);
                l3.entries[l3i].set(l3.entries[l3i].addr(), f);
                l3.entries[l3i].addr()
            } else {
                let t = frame_alloc.allocate_frame().ok_or("OOM: L2 table")?;
                l3.entries[l3i].set(t, pf);
                self.get_table(t).zero(); t
            };

            // L2 → L1
            let l2 = self.get_table(l2_phys);
            let l1_phys = if l2.entries[l2i].is_present() {
                let mut f = l2.entries[l2i].flags(); f.merge(pf);
                l2.entries[l2i].set(l2.entries[l2i].addr(), f);
                l2.entries[l2i].addr()
            } else {
                let t = frame_alloc.allocate_frame().ok_or("OOM: L1 table")?;
                l2.entries[l2i].set(t, pf);
                self.get_table(t).zero(); t
            };

            // Leaf
            let l1 = self.get_table(l1_phys);
            if l1.entries[l1i].is_present() { return Err("Page already mapped"); }
            l1.entries[l1i].set(phys, flags);
            core::arch::asm!("invlpg [{}]", in(reg) virt);
        }
        Ok(())
    }

    /// Remove a leaf mapping and return the freed physical address.
    unsafe fn unmap(
        &mut self,
        virt:        u64,
        frame_alloc: &mut PhysicalFrameAllocator,
    ) -> Result<u64, &'static str> {
        let l4i = ((virt >> 39) & 0x1FF) as usize;
        let l3i = ((virt >> 30) & 0x1FF) as usize;
        let l2i = ((virt >> 21) & 0x1FF) as usize;
        let l1i = ((virt >> 12) & 0x1FF) as usize;

        unsafe {
            let l4 = self.get_table(self.l4_table_phys);
            if !l4.entries[l4i].is_present() { return Err("Not mapped (L4)"); }

            let l3 = self.get_table(l4.entries[l4i].addr());
            if !l3.entries[l3i].is_present() { return Err("Not mapped (L3)"); }

            let l2 = self.get_table(l3.entries[l3i].addr());
            if !l2.entries[l2i].is_present() { return Err("Not mapped (L2)"); }

            let l1 = self.get_table(l2.entries[l2i].addr());
            if !l1.entries[l1i].is_present() { return Err("Not mapped (L1)"); }

            let phys = l1.entries[l1i].addr();
            l1.entries[l1i].clear();
            core::arch::asm!("invlpg [{}]", in(reg) virt);
            frame_alloc.free_frame(phys);
            Ok(phys)
        }
    }
}

// ============================================================================
// PAGING ALLOCATOR
// ============================================================================

const FREE_LIST_CAPACITY: usize = 256;

struct PagingAllocatorInner {
    frame_allocator:    PhysicalFrameAllocator,
    page_table_manager: Option<PageTableManager>,
    next_virt_addr:     AtomicUsize,
    heap_start:         usize,
    heap_end:           usize,
    initialized:        AtomicBool,
    /// Recycled virtual ranges: (virt_start, num_pages)
    free_list:     [(usize, usize); FREE_LIST_CAPACITY],
    free_list_len: usize,
}

pub struct PagingAllocator {
    inner: UnsafeCell<PagingAllocatorInner>,
}

unsafe impl Sync for PagingAllocator {}

impl PagingAllocator {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(PagingAllocatorInner {
                frame_allocator:    PhysicalFrameAllocator::new(),
                page_table_manager: None,
                next_virt_addr:     AtomicUsize::new(0),
                heap_start:         0,
                heap_end:           0,
                initialized:        AtomicBool::new(false),
                free_list:          [(0, 0); FREE_LIST_CAPACITY],
                free_list_len:      0,
            }),
        }
    }

    pub unsafe fn init(&self, memory_map: &MemoryMapRequest) {
        let inner = unsafe { &mut *self.inner.get() };

        unsafe { SERIAL_PORT.write_str("=== INITIALIZING PAGING ALLOCATOR ===\n") };
        unsafe { inner.frame_allocator.init(memory_map) };

        inner.page_table_manager = Some(PageTableManager::new(0xFFFF800000000000));

        inner.heap_start = 0xFFFFFF0000000000;
        inner.heap_end   = inner.heap_start + (64 * 1024 * 1024); // 64 MB
        inner.next_virt_addr.store(inner.heap_start, Ordering::Relaxed);

        unsafe {
            SERIAL_PORT.write_str("Heap: 0x");
            SERIAL_PORT.write_hex((inner.heap_start >> 32) as u32);
            SERIAL_PORT.write_hex(inner.heap_start as u32);
            SERIAL_PORT.write_str(" - 0x");
            SERIAL_PORT.write_hex((inner.heap_end >> 32) as u32);
            SERIAL_PORT.write_hex(inner.heap_end as u32);
            SERIAL_PORT.write_str("\n");
        }

        inner.initialized.store(true, Ordering::Relaxed);
        unsafe { SERIAL_PORT.write_str("=== PAGING ALLOCATOR READY ===\n") };
    }

    unsafe fn allocate_pages(&self, num_pages: usize) -> Option<NonNull<u8>> {
        let inner = unsafe { &mut *self.inner.get() };
        if !inner.initialized.load(Ordering::Relaxed) { return None; }

        // ── 1. Check free list for exact-size recycled range ─────────────────
        for i in 0..inner.free_list_len {
            let (fl_virt, fl_pages) = inner.free_list[i];
            if fl_pages == num_pages {
                // Swap-remove from free list
                inner.free_list[i] = inner.free_list[inner.free_list_len - 1];
                inner.free_list_len -= 1;

                let ptm = inner.page_table_manager.as_mut()?;
                for j in 0..num_pages {
                    let virt = (fl_virt + j * 4096) as u64;
                    let phys = inner.frame_allocator.allocate_frame()?;
                    unsafe {
                        if ptm.map(virt, phys, PageTableFlags::kernel_flags(),
                                   &mut inner.frame_allocator).is_err() {
                            return None;
                        }
                    }
                }
                return NonNull::new(fl_virt as *mut u8);
            }
        }

        // ── 2. Bump-allocate fresh virtual range ─────────────────────────────
        let ptm = inner.page_table_manager.as_mut()?;
        let virt_start = inner.next_virt_addr.fetch_add(num_pages * 4096, Ordering::Relaxed);

        if virt_start + num_pages * 4096 > inner.heap_end {
            unsafe { SERIAL_PORT.write_str("PAGING ALLOCATOR: Heap exhausted!\n") };
            return None;
        }

        for i in 0..num_pages {
            let virt = (virt_start + i * 4096) as u64;
            let phys = match inner.frame_allocator.allocate_frame() {
                Some(a) => a,
                None => {
                    unsafe { SERIAL_PORT.write_str("PAGING ALLOCATOR: Out of frames!\n") };
                    return None;
                }
            };
            unsafe {
                if let Err(e) = ptm.map(virt, phys, PageTableFlags::kernel_flags(),
                                        &mut inner.frame_allocator) {
                    SERIAL_PORT.write_str("PAGING ALLOCATOR: Map failed: ");
                    SERIAL_PORT.write_str(e);
                    SERIAL_PORT.write_str("\n");
                    return None;
                }
            }
        }

        NonNull::new(virt_start as *mut u8)
    }
}

unsafe impl GlobalAlloc for PagingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let num_pages = (layout.size() + 4095) / 4096;
        match unsafe { self.allocate_pages(num_pages) } {
            Some(p) => p.as_ptr(),
            None    => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let inner = unsafe { &mut *self.inner.get() };
        if !inner.initialized.load(Ordering::Relaxed) { return; }

        let virt_start = ptr as usize;
        let num_pages  = (layout.size() + 4095) / 4096;

        let ptm = match inner.page_table_manager.as_mut() {
            Some(p) => p,
            None    => return,
        };

        // Unmap every page — this frees the backing physical frame each time
        for i in 0..num_pages {
            let virt = (virt_start + i * 4096) as u64;
            let _ = unsafe { ptm.unmap(virt, &mut inner.frame_allocator) };
        }

        // Push virtual range into the free list for reuse
        if inner.free_list_len < FREE_LIST_CAPACITY {
            inner.free_list[inner.free_list_len] = (virt_start, num_pages);
            inner.free_list_len += 1;
        }
        // If the list is full the VA range is simply abandoned — with a 64 MB
        // heap and 256-entry list this is extremely unlikely in practice.
    }
}

// ============================================================================
// GLOBAL ALLOCATOR INSTANCE
// ============================================================================

#[global_allocator]
pub static ALLOCATOR: PagingAllocator = PagingAllocator::new();

pub unsafe fn init_paging_heap(memory_map: &MemoryMapRequest) {
    unsafe { ALLOCATOR.init(memory_map) };
}

// ── User-space region helpers (used by user_mode.rs) ─────────────────────

pub unsafe fn map_user_region(
    virt_addr:  u64,
    num_pages:  usize,
    writable:   bool,
    executable: bool,
) -> Result<(), &'static str> {
    let inner = unsafe { &mut *ALLOCATOR.inner.get() };
    if !inner.initialized.load(Ordering::Relaxed) {
        return Err("Paging allocator not initialized");
    }
    let ptm   = inner.page_table_manager.as_mut().ok_or("PTM unavailable")?;
    let flags = PageTableFlags::user_flags(writable, executable);

    for page in 0..num_pages {
        let page_virt = virt_addr + (page * 4096) as u64;
        let phys      = inner.frame_allocator.allocate_frame().ok_or("Out of frames")?;
        unsafe {
            ptm.map(page_virt, phys, flags, &mut inner.frame_allocator)?;
            core::ptr::write_bytes(page_virt as *mut u8, 0, 4096);
        }
    }
    Ok(())
}

pub unsafe fn copy_to_region(dest: u64, bytes: &[u8]) {
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dest as *mut u8, bytes.len()) };
}

// ── Per-process page-table helpers ───────────────────────────────────────────

/// Create a new user page table.
///
/// Allocates a fresh L4 table, zeroes the lower half (user space, indices
/// 0–255) and copies the kernel higher-half entries (indices 256–511) from
/// the current kernel L4 so that kernel code and interrupt handlers remain
/// accessible while the task runs.  Returns the physical address of the new
/// L4 table (to be loaded into CR3).
pub unsafe fn create_user_page_table() -> Option<u64> {
    let inner = unsafe { &mut *ALLOCATOR.inner.get() };
    if !inner.initialized.load(Ordering::Relaxed) { return None; }

    // Capture what we need before borrowing frame_allocator (field split).
    let hho          = inner.page_table_manager.as_ref()?.higher_half_offset;
    let kernel_l4_pa = inner.page_table_manager.as_ref()?.l4_table_phys;

    let new_l4_pa = inner.frame_allocator.allocate_frame()?;

    let new_l4    = (new_l4_pa    + hho) as *mut u64;
    let kernel_l4 = (kernel_l4_pa + hho) as *const u64;

    unsafe {
        // Zero user half
        for i in 0..256usize   { *new_l4.add(i) = 0; }
        // Clone kernel half
        for i in 256..512usize { *new_l4.add(i) = *kernel_l4.add(i); }
    }
    Some(new_l4_pa)
}

/// Map user-space pages into the page table identified by `cr3_phys` WITHOUT
/// switching the CPU's CR3.
///
/// Physical frames are zeroed via the higher-half physical window (always
/// accessible in all page tables).  Intermediate page-table nodes (L3/L2/L1)
/// are allocated from the global frame allocator and installed in `cr3_phys`.
pub unsafe fn map_user_region_in(
    cr3_phys:   u64,
    virt_addr:  u64,
    num_pages:  usize,
    writable:   bool,
    executable: bool,
) -> Result<(), &'static str> {
    let inner = unsafe { &mut *ALLOCATOR.inner.get() };
    if !inner.initialized.load(Ordering::Relaxed) {
        return Err("Paging allocator not initialized");
    }

    let flags  = PageTableFlags::user_flags(writable, executable);
    let ptm    = inner.page_table_manager.as_mut().ok_or("PTM unavailable")?;
    let hho    = ptm.higher_half_offset;
    let saved  = ptm.l4_table_phys;
    ptm.l4_table_phys = cr3_phys;

    let mut result: Result<(), &'static str> = Ok(());
    for page in 0..num_pages {
        let page_virt = virt_addr + (page * 4096) as u64;
        let phys = match inner.frame_allocator.allocate_frame() {
            Some(p) => p,
            None    => { result = Err("Out of frames"); break; }
        };
        // Zero the physical frame via the higher-half identity window.
        unsafe { core::ptr::write_bytes((phys + hho) as *mut u8, 0, 4096); }
        if let Err(e) = unsafe {
            ptm.map(page_virt, phys, flags, &mut inner.frame_allocator)
        } {
            result = Err(e); break;
        }
    }

    // Always restore the kernel L4 pointer.
    inner.page_table_manager.as_mut().unwrap().l4_table_phys = saved;
    result
}

/// Walk the user-space half (L4 indices 0–255) of the page table at
/// `cr3_phys`, free every mapped leaf frame and every intermediate
/// page-table frame, then free the L4 frame itself.
///
/// The kernel higher-half (indices 256–511) is **not** touched — those
/// entries are shared pointers into the kernel's own page table.
///
/// Call this after switching away from the task's CR3; the function is safe
/// because all physical frames are accessed via the higher-half identity
/// window, not through the task's own virtual mappings.
pub unsafe fn free_user_page_table(cr3_phys: u64) {
    let inner = unsafe { &mut *ALLOCATOR.inner.get() };
    if !inner.initialized.load(Ordering::Relaxed) { return; }
    let hho = match inner.page_table_manager.as_ref() {
        Some(ptm) => ptm.higher_half_offset,
        None => return,
    };
    let fa = &mut inner.frame_allocator;

    let l4 = (cr3_phys + hho) as *const u64;
    for l4i in 0..256usize {
        let l4e = unsafe { *l4.add(l4i) };
        if l4e & 1 == 0 { continue; }
        let l3_phys = l4e & 0x000F_FFFF_FFFF_F000;

        let l3 = (l3_phys + hho) as *const u64;
        for l3i in 0..512usize {
            let l3e = unsafe { *l3.add(l3i) };
            if l3e & 1 == 0 { continue; }
            let l2_phys = l3e & 0x000F_FFFF_FFFF_F000;

            let l2 = (l2_phys + hho) as *const u64;
            for l2i in 0..512usize {
                let l2e = unsafe { *l2.add(l2i) };
                if l2e & 1 == 0 { continue; }
                let l1_phys = l2e & 0x000F_FFFF_FFFF_F000;

                let l1 = (l1_phys + hho) as *const u64;
                for l1i in 0..512usize {
                    let l1e = unsafe { *l1.add(l1i) };
                    if l1e & 1 == 0 { continue; }
                    fa.free_frame(l1e & 0x000F_FFFF_FFFF_F000);
                }
                fa.free_frame(l1_phys); // free the L1 table frame
            }
            fa.free_frame(l2_phys); // free the L2 table frame
        }
        fa.free_frame(l3_phys); // free the L3 table frame
    }
    fa.free_frame(cr3_phys); // free the L4 frame
}

/// Make a full physical copy of the user-space half (L4 indices 0–255) of the
/// page table at `src_cr3`.  Every mapped leaf frame is duplicated into a
/// freshly-allocated frame; new L3/L2/L1 table frames are allocated as well.
/// The kernel half (indices 256–511) is copied as shared pointers (same as
/// `create_user_page_table`).  Returns the new L4 physical address, or `None`
/// if physical memory is exhausted.
pub unsafe fn copy_user_page_table(src_cr3: u64) -> Option<u64> {
    let inner = unsafe { &mut *ALLOCATOR.inner.get() };
    if !inner.initialized.load(Ordering::Relaxed) { return None; }
    let hho        = inner.page_table_manager.as_ref()?.higher_half_offset;
    let kernel_l4  = inner.page_table_manager.as_ref()?.l4_table_phys;
    let fa         = &mut inner.frame_allocator;

    let dst_l4_pa  = fa.allocate_frame()?;
    let src_l4     = (src_cr3   + hho) as *const u64;
    let dst_l4     = (dst_l4_pa + hho) as *mut   u64;
    let kern_l4    = (kernel_l4 + hho) as *const u64;

    unsafe {
        for i in 0..256usize  { *dst_l4.add(i) = 0; }
        for i in 256..512usize { *dst_l4.add(i) = *kern_l4.add(i); }
    }

    for l4i in 0..256usize {
        let l4e = unsafe { *src_l4.add(l4i) };
        if l4e & 1 == 0 { continue; }
        let src_l3 = (l4e & 0x000F_FFFF_FFFF_F000 + hho) as *const u64;
        let dst_l3_pa = fa.allocate_frame()?;
        let dst_l3    = (dst_l3_pa + hho) as *mut u64;
        unsafe {
            core::ptr::write_bytes(dst_l3 as *mut u8, 0, 4096);
            *dst_l4.add(l4i) = dst_l3_pa | (l4e & 0xFFF);
        }

        for l3i in 0..512usize {
            let l3e = unsafe { *src_l3.add(l3i) };
            if l3e & 1 == 0 { continue; }
            let src_l2 = ((l3e & 0x000F_FFFF_FFFF_F000) + hho) as *const u64;
            let dst_l2_pa = fa.allocate_frame()?;
            let dst_l2    = (dst_l2_pa + hho) as *mut u64;
            unsafe {
                core::ptr::write_bytes(dst_l2 as *mut u8, 0, 4096);
                *dst_l3.add(l3i) = dst_l2_pa | (l3e & 0xFFF);
            }

            for l2i in 0..512usize {
                let l2e = unsafe { *src_l2.add(l2i) };
                if l2e & 1 == 0 { continue; }
                let src_l1 = ((l2e & 0x000F_FFFF_FFFF_F000) + hho) as *const u64;
                let dst_l1_pa = fa.allocate_frame()?;
                let dst_l1    = (dst_l1_pa + hho) as *mut u64;
                unsafe {
                    core::ptr::write_bytes(dst_l1 as *mut u8, 0, 4096);
                    *dst_l2.add(l2i) = dst_l1_pa | (l2e & 0xFFF);
                }

                for l1i in 0..512usize {
                    let l1e = unsafe { *src_l1.add(l1i) };
                    if l1e & 1 == 0 { continue; }
                    let src_frame = (l1e & 0x000F_FFFF_FFFF_F000) + hho;
                    let dst_frame_pa = fa.allocate_frame()?;
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            src_frame as *const u8,
                            (dst_frame_pa + hho) as *mut u8,
                            4096,
                        );
                        *dst_l1.add(l1i) = dst_frame_pa | (l1e & 0xFFF);
                    }
                }
            }
        }
    }
    Some(dst_l4_pa)
}

/// Copy bytes into virtual address `dest` inside the page table `cr3_phys`.
///
/// Temporarily switches the CPU's CR3 to `cr3_phys`, performs the copy, then
/// restores the original CR3.  Safe because the kernel higher-half is mapped
/// identically in every page table.
pub unsafe fn copy_to_region_in(cr3_phys: u64, dest: u64, bytes: &[u8]) {
    let saved: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) saved, options(nostack, nomem));
        core::arch::asm!("mov cr3, {}", in(reg) cr3_phys, options(nostack, nomem));
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), dest as *mut u8, bytes.len());
        core::arch::asm!("mov cr3, {}", in(reg) saved, options(nostack, nomem));
    }
}
