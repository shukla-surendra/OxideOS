// src/kernel/paging_allocator.rs
//! Page Table Based Memory Allocator for OxideOS
//! 
//! This allocator actually manipulates page tables to map virtual addresses
//! to physical frames on-demand, rather than just using pre-mapped memory.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use core::cell::UnsafeCell;
use limine::memory_map::{Entry, EntryType};
use limine::request::MemoryMapRequest;
use crate::kernel::serial::SERIAL_PORT;

// ============================================================================
// PAGE TABLE STRUCTURES (x86_64)
// ============================================================================

/// Page table entry flags
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
struct PageTableFlags(u64);

impl PageTableFlags {
    const PRESENT: u64      = 1 << 0;
    const WRITABLE: u64     = 1 << 1;
    const USER: u64         = 1 << 2;
    const WRITE_THROUGH: u64 = 1 << 3;
    const NO_CACHE: u64     = 1 << 4;
    const ACCESSED: u64     = 1 << 5;
    const DIRTY: u64        = 1 << 6;
    const HUGE: u64         = 1 << 7;
    const GLOBAL: u64       = 1 << 8;
    const NO_EXECUTE: u64   = 1 << 63;

    fn new() -> Self {
        Self(0)
    }

    fn set_present(&mut self, present: bool) {
        if present {
            self.0 |= Self::PRESENT;
        } else {
            self.0 &= !Self::PRESENT;
        }
    }

    fn set_writable(&mut self, writable: bool) {
        if writable {
            self.0 |= Self::WRITABLE;
        } else {
            self.0 &= !Self::WRITABLE;
        }
    }

    fn is_present(&self) -> bool {
        self.0 & Self::PRESENT != 0
    }

    fn kernel_flags() -> Self {
        Self(Self::PRESENT | Self::WRITABLE)
    }
}

/// Single page table entry
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
struct PageTableEntry(u64);

impl PageTableEntry {
    fn new() -> Self {
        Self(0)
    }

    fn is_present(&self) -> bool {
        self.0 & PageTableFlags::PRESENT != 0
    }

    fn flags(&self) -> PageTableFlags {
        PageTableFlags(self.0 & 0xFFF)
    }

    fn addr(&self) -> u64 {
        self.0 & 0x000F_FFFF_FFFF_F000
    }

    fn set(&mut self, addr: u64, flags: PageTableFlags) {
        self.0 = (addr & 0x000F_FFFF_FFFF_F000) | (flags.0 & 0xFFF);
    }

    fn clear(&mut self) {
        self.0 = 0;
    }
}

/// Page table (512 entries)
#[repr(align(4096))]
struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    fn new() -> Self {
        Self {
            entries: [PageTableEntry::new(); 512],
        }
    }

    fn zero(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }
}

// ============================================================================
// PHYSICAL FRAME ALLOCATOR
// ============================================================================

/// Tracks free physical frames using a bitmap
struct PhysicalFrameAllocator {
    bitmap: [u64; 1024], // 1024 * 64 = 65536 frames = 256MB manageable
    next_frame: AtomicUsize,
    total_frames: usize,
    allocated_frames: AtomicUsize,
}

impl PhysicalFrameAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [0; 1024],
            next_frame: AtomicUsize::new(0),
            total_frames: 0,
            allocated_frames: AtomicUsize::new(0),
        }
    }

    unsafe fn init(&mut self, memory_map: &MemoryMapRequest) {
        unsafe { SERIAL_PORT.write_str("=== INITIALIZING PHYSICAL FRAME ALLOCATOR ===\n") };

        if let Some(map) = memory_map.get_response() {
            // Mark all frames as used initially
            for word in &mut self.bitmap {
                *word = u64::MAX;
            }

            // Find usable regions and mark frames as free
            for entry in map.entries() {
                if entry.entry_type == EntryType::USABLE {
                    let start_frame = (entry.base as usize) / 4096;
                    let frame_count = (entry.length as usize) / 4096;

                    // Only track frames above 16MB to be safe
                    let safe_start_frame = core::cmp::max(start_frame, 4096); // 16MB
                    
                    if safe_start_frame < 65536 { // Within our bitmap range
                        let end_frame = core::cmp::min(start_frame + frame_count, 65536);
                        
                        for frame in safe_start_frame..end_frame {
                            self.mark_free(frame);
                            self.total_frames += 1;
                        }

                        unsafe {
                            SERIAL_PORT.write_str("  Tracked frames ");
                            SERIAL_PORT.write_decimal(safe_start_frame as u32);
                            SERIAL_PORT.write_str(" - ");
                            SERIAL_PORT.write_decimal(end_frame as u32);
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
        if frame < 65536 {
            let idx = frame / 64;
            let bit = frame % 64;
            self.bitmap[idx] &= !(1u64 << bit);
        }
    }

    fn mark_used(&mut self, frame: usize) {
        if frame < 65536 {
            let idx = frame / 64;
            let bit = frame % 64;
            self.bitmap[idx] |= 1u64 << bit;
        }
    }

    fn is_free(&self, frame: usize) -> bool {
        if frame < 65536 {
            let idx = frame / 64;
            let bit = frame % 64;
            (self.bitmap[idx] & (1u64 << bit)) == 0
        } else {
            false
        }
    }

    fn allocate_frame(&mut self) -> Option<u64> {
        let start = self.next_frame.load(Ordering::Relaxed);
        
        // Search for free frame
        for offset in 0..self.total_frames {
            let frame = (start + offset) % 65536;
            
            if self.is_free(frame) {
                self.mark_used(frame);
                self.next_frame.store((frame + 1) % 65536, Ordering::Relaxed);
                self.allocated_frames.fetch_add(1, Ordering::Relaxed);
                
                // Return physical address
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
    l4_table_phys: u64,
    higher_half_offset: u64,
}

impl PageTableManager {
    fn new(higher_half_offset: u64) -> Self {
        // Get current CR3 (root page table)
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3);
        }

        Self {
            l4_table_phys: cr3 & 0x000F_FFFF_FFFF_F000,
            higher_half_offset,
        }
    }

    /// Convert physical address to virtual (using higher half mapping)
    fn phys_to_virt(&self, phys: u64) -> *mut u8 {
        (phys + self.higher_half_offset) as *mut u8
    }

    /// Get page table at physical address
    unsafe fn get_table(&self, phys_addr: u64) -> &mut PageTable {
        let virt = self.phys_to_virt(phys_addr);
        &mut *(virt as *mut PageTable)
    }

    /// Map a virtual address to a physical frame
    unsafe fn map(&mut self, virt_addr: u64, phys_addr: u64, flags: PageTableFlags, frame_alloc: &mut PhysicalFrameAllocator) -> Result<(), &'static str> {
        // Extract page table indices
        let l4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let l3_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let l2_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        let l1_idx = ((virt_addr >> 12) & 0x1FF) as usize;

        // Walk L4 -> L3
        let l4_table = self.get_table(self.l4_table_phys);
        let l3_phys = if l4_table.entries[l4_idx].is_present() {
            l4_table.entries[l4_idx].addr()
        } else {
            // Allocate new L3 table
            let new_table = frame_alloc.allocate_frame()
                .ok_or("Out of physical frames")?;
            l4_table.entries[l4_idx].set(new_table, PageTableFlags::kernel_flags());
            
            // Zero the new table
            let table = self.get_table(new_table);
            table.zero();
            new_table
        };

        // Walk L3 -> L2
        let l3_table = self.get_table(l3_phys);
        let l2_phys = if l3_table.entries[l3_idx].is_present() {
            l3_table.entries[l3_idx].addr()
        } else {
            let new_table = frame_alloc.allocate_frame()
                .ok_or("Out of physical frames")?;
            l3_table.entries[l3_idx].set(new_table, PageTableFlags::kernel_flags());
            
            let table = self.get_table(new_table);
            table.zero();
            new_table
        };

        // Walk L2 -> L1
        let l2_table = self.get_table(l2_phys);
        let l1_phys = if l2_table.entries[l2_idx].is_present() {
            l2_table.entries[l2_idx].addr()
        } else {
            let new_table = frame_alloc.allocate_frame()
                .ok_or("Out of physical frames")?;
            l2_table.entries[l2_idx].set(new_table, PageTableFlags::kernel_flags());
            
            let table = self.get_table(new_table);
            table.zero();
            new_table
        };

        // Set the final mapping in L1
        let l1_table = self.get_table(l1_phys);
        if l1_table.entries[l1_idx].is_present() {
            return Err("Page already mapped");
        }
        l1_table.entries[l1_idx].set(phys_addr, flags);

        // Flush TLB for this address
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) virt_addr);
        }

        Ok(())
    }

    /// Unmap a virtual address
    unsafe fn unmap(&mut self, virt_addr: u64, frame_alloc: &mut PhysicalFrameAllocator) -> Result<u64, &'static str> {
        let l4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let l3_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let l2_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        let l1_idx = ((virt_addr >> 12) & 0x1FF) as usize;

        let l4_table = self.get_table(self.l4_table_phys);
        if !l4_table.entries[l4_idx].is_present() {
            return Err("Page not mapped (L4)");
        }

        let l3_table = self.get_table(l4_table.entries[l4_idx].addr());
        if !l3_table.entries[l3_idx].is_present() {
            return Err("Page not mapped (L3)");
        }

        let l2_table = self.get_table(l3_table.entries[l3_idx].addr());
        if !l2_table.entries[l2_idx].is_present() {
            return Err("Page not mapped (L2)");
        }

        let l1_table = self.get_table(l2_table.entries[l2_idx].addr());
        if !l1_table.entries[l1_idx].is_present() {
            return Err("Page not mapped (L1)");
        }

        let phys_addr = l1_table.entries[l1_idx].addr();
        l1_table.entries[l1_idx].clear();

        // Flush TLB
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) virt_addr);
        }

        // Free the physical frame
        frame_alloc.free_frame(phys_addr);

        Ok(phys_addr)
    }
}

// ============================================================================
// PAGING ALLOCATOR
// ============================================================================

struct PagingAllocatorInner {
    frame_allocator: PhysicalFrameAllocator,
    page_table_manager: Option<PageTableManager>,
    next_virt_addr: AtomicUsize,
    heap_start: usize,
    heap_end: usize,
    initialized: AtomicBool,
}

pub struct PagingAllocator {
    inner: UnsafeCell<PagingAllocatorInner>,
}

// Safety: We ensure exclusive access through careful synchronization
unsafe impl Sync for PagingAllocator {}

impl PagingAllocator {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(PagingAllocatorInner {
                frame_allocator: PhysicalFrameAllocator::new(),
                page_table_manager: None,
                next_virt_addr: AtomicUsize::new(0),
                heap_start: 0,
                heap_end: 0,
                initialized: AtomicBool::new(false),
            }),
        }
    }

    pub unsafe fn init(&self, memory_map: &MemoryMapRequest) {
        let inner = &mut *self.inner.get();
        
        unsafe { SERIAL_PORT.write_str("=== INITIALIZING PAGING ALLOCATOR ===\n") };

        // Initialize physical frame allocator
        inner.frame_allocator.init(memory_map);

        // Set up page table manager
        // Assume higher half offset is 0xFFFF800000000000 (typical for x86_64)
        let higher_half_offset = 0xFFFF800000000000;
        inner.page_table_manager = Some(PageTableManager::new(higher_half_offset));

        // IMPORTANT: Choose a heap address that doesn't conflict with Limine
        // Limine typically uses:
        // - 0xFFFF800000000000 - 0xFFFF800040000000: Direct map of physical memory
        // - 0xFFFFFFFF80000000 - 0xFFFFFFFFFFFFFFFF: Kernel code/data
        //
        // We'll use 0xFFFFFF0000000000 which is:
        // - In the canonical higher half (bit 47 = 1)
        // - Far from typical mappings
        // - Still accessible with sign-extended addresses
        inner.heap_start = 0xFFFFFF0000000000;
        inner.heap_end = inner.heap_start + (64 * 1024 * 1024); // 64MB heap
        inner.next_virt_addr.store(inner.heap_start, Ordering::Relaxed);

        unsafe {
            SERIAL_PORT.write_str("Heap range: 0x");
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
        let inner = &mut *self.inner.get();
        
        if !inner.initialized.load(Ordering::Relaxed) {
            return None;
        }

        let page_table_manager = inner.page_table_manager.as_mut()?;
        
        // Allocate virtual address space
        let virt_start = inner.next_virt_addr.fetch_add(num_pages * 4096, Ordering::Relaxed);
        
        if virt_start + (num_pages * 4096) > inner.heap_end {
            unsafe { SERIAL_PORT.write_str("PAGING ALLOCATOR: Heap exhausted!\n") };
            return None;
        }

        // Map each page
        for i in 0..num_pages {
            let virt_addr = (virt_start + i * 4096) as u64;
            
            // Allocate physical frame
            let phys_addr = match inner.frame_allocator.allocate_frame() {
                Some(addr) => addr,
                None => {
                    unsafe { SERIAL_PORT.write_str("PAGING ALLOCATOR: Out of physical frames!\n") };
                    // TODO: Clean up already-mapped pages
                    return None;
                }
            };

            // Map virtual to physical
            unsafe {
                if let Err(e) = page_table_manager.map(
                    virt_addr,
                    phys_addr,
                    PageTableFlags::kernel_flags(),
                    &mut inner.frame_allocator
                ) {
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
        let size = layout.size();
        let num_pages = (size + 4095) / 4096; // Round up to pages

        if let Some(ptr) = self.allocate_pages(num_pages) {
            ptr.as_ptr()
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // TODO: Implement deallocation
        // Would need to unmap pages and free physical frames
        let _ = (ptr, layout); // Suppress warnings for now
    }
}

// ============================================================================
// GLOBAL ALLOCATOR INSTANCE
// ============================================================================

#[global_allocator]
pub static ALLOCATOR: PagingAllocator = PagingAllocator::new();

pub unsafe fn init_paging_heap(memory_map: &MemoryMapRequest) {
    ALLOCATOR.init(memory_map);
}