// src/kernel/allocator.rs
//! Memory Allocator for OxideOS
//! 
//! This module provides a simple bump allocator for kernel heap allocation.
//! It parses the Limine memory map to find usable memory regions.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering};
use limine::memory_map::{Entry, EntryType};
use limine::request::MemoryMapRequest;
use crate::kernel::serial::SERIAL_PORT;

// ============================================================================
// MEMORY REGION TRACKING
// ============================================================================

#[derive(Debug, Copy, Clone)]
struct MemoryRegion {
    start: usize,
    end: usize,
    used: usize,
}

impl MemoryRegion {
    fn new(start: usize, size: usize) -> Self {
        Self {
            start,
            end: start + size,
            used: 0,
        }
    }

    fn allocate(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        // Calculate aligned start address
        let current = self.start + self.used;
        let aligned = (current + align - 1) & !(align - 1);
        
        // Check if allocation fits
        if aligned + size <= self.end {
            self.used = aligned + size - self.start;
            NonNull::new(aligned as *mut u8)
        } else {
            None
        }
    }

    fn available(&self) -> usize {
        self.end - (self.start + self.used)
    }
}

// ============================================================================
// BUMP ALLOCATOR
// ============================================================================

pub struct BumpAllocator {
    regions: [Option<MemoryRegion>; 8], // Support up to 8 memory regions
    current_region: AtomicUsize,
    total_allocated: AtomicUsize,
    total_available: AtomicUsize,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        Self {
            regions: [None; 8],
            current_region: AtomicUsize::new(0),
            total_allocated: AtomicUsize::new(0),
            total_available: AtomicUsize::new(0),
        }
    }

    /// Initialize the allocator with memory regions from Limine
    pub unsafe fn init(&mut self, memory_map_request: &MemoryMapRequest) {
        unsafe{SERIAL_PORT.write_str("=== INITIALIZING MEMORY ALLOCATOR ===\n")};

        if let Some(memory_map) = memory_map_request.get_response() {
            let mut region_count = 0;
            let mut total_memory = 0usize;
            let mut usable_memory = 0usize;

            // Find usable memory regions
            for entry in memory_map.entries() {
                total_memory += entry.length as usize;

                // Only use USABLE memory regions
                if entry.entry_type == EntryType::USABLE {
                    let start = entry.base as usize;
                    let size = entry.length as usize;
                    unsafe{
                        SERIAL_PORT.write_str("  Usable region: 0x");
                        SERIAL_PORT.write_hex((start >> 32) as u32);
                        SERIAL_PORT.write_hex(start as u32);
                        SERIAL_PORT.write_str(" - 0x");
                        SERIAL_PORT.write_hex(((start + size) >> 32) as u32);
                        SERIAL_PORT.write_hex((start + size) as u32);
                        SERIAL_PORT.write_str(" (");
                        SERIAL_PORT.write_decimal((size / 1024) as u32);
                        SERIAL_PORT.write_str(" KB)\n");
                    }

                    // CRITICAL: Only use memory above 8MB to avoid bootloader/kernel conflicts
                    // This is much more reasonable than 64MB
                    let min_safe_address = 0x800000; // 8MB
                    
                    // Skip regions that are too small 
                    if size < 0x100000 {  // Still require 1MB minimum
                        SERIAL_PORT.write_str("    Skipping (too small - need 1MB minimum)\n");
                        continue;
                    }

                    // For regions that start below our minimum, see if part of them is usable
                    let safe_start = core::cmp::max(start, min_safe_address);
                    let region_end = start + size;
                    
                    // Skip if the entire region is below our threshold
                    if region_end <= min_safe_address {
                        SERIAL_PORT.write_str("    Skipping (entirely below 8MB)\n");
                        continue;
                    }

                    // Add region to our allocator - only use the safe part
                    if region_count < 8 {
                        // Use only the part of the region that's above our minimum
                        let safe_start = core::cmp::max(start, min_safe_address);
                        let safe_end = start + size;
                        
                        if safe_end > safe_start {
                            let safe_size = safe_end - safe_start;
                            
                            // Align to page boundaries
                            let aligned_start = (safe_start + 4095) & !4095;
                            let aligned_size = (safe_size / 4096) * 4096;

                            if aligned_size >= 0x100000 {  // At least 1MB
                                self.regions[region_count] = Some(MemoryRegion::new(
                                    aligned_start,
                                    aligned_size
                                ));
                                usable_memory += aligned_size;
                                region_count += 1;

                                SERIAL_PORT.write_str("    Added as heap region #");
                                SERIAL_PORT.write_decimal(region_count as u32);
                                SERIAL_PORT.write_str(" at 0x");
                                SERIAL_PORT.write_hex((aligned_start >> 32) as u32);
                                SERIAL_PORT.write_hex(aligned_start as u32);
                                SERIAL_PORT.write_str(" size 0x");
                                SERIAL_PORT.write_hex((aligned_size >> 32) as u32);
                                SERIAL_PORT.write_hex(aligned_size as u32);
                                SERIAL_PORT.write_str("\n");
                            } else {
                                SERIAL_PORT.write_str("    Skipping (safe portion too small)\n");
                            }
                        } else {
                            SERIAL_PORT.write_str("    Skipping (no safe portion available)\n");
                        }
                    }
                }
            }

            self.total_available.store(usable_memory, Ordering::Relaxed);

            SERIAL_PORT.write_str("Memory summary:\n");
            SERIAL_PORT.write_str("  Total system memory: ");
            SERIAL_PORT.write_decimal((total_memory / 1024 / 1024) as u32);
            SERIAL_PORT.write_str(" MB\n");
            SERIAL_PORT.write_str("  Usable for heap: ");
            SERIAL_PORT.write_decimal((usable_memory / 1024 / 1024) as u32);
            SERIAL_PORT.write_str(" MB\n");
            SERIAL_PORT.write_str("  Active regions: ");
            SERIAL_PORT.write_decimal(region_count as u32);
            SERIAL_PORT.write_str("\n");

            if region_count == 0 {
                SERIAL_PORT.write_str("ERROR: No usable memory regions found!\n");
                panic!("No usable memory for heap allocator");
            }
        } else {
            SERIAL_PORT.write_str("ERROR: Failed to get memory map from Limine\n");
            panic!("Cannot initialize allocator without memory map");
        }

        SERIAL_PORT.write_str("=== ALLOCATOR INITIALIZATION COMPLETE ===\n");
    }

    /// Get allocation statistics
    pub fn stats(&self) -> (usize, usize, usize) {
        let allocated = self.total_allocated.load(Ordering::Relaxed);
        let available = self.total_available.load(Ordering::Relaxed);
        let free = available.saturating_sub(allocated);
        (allocated, free, available)
    }

    /// Print detailed allocator information
    pub unsafe fn debug_info(&self) {
        SERIAL_PORT.write_str("=== ALLOCATOR DEBUG INFO ===\n");
        
        let (allocated, free, total) = self.stats();
        SERIAL_PORT.write_str("Memory usage:\n");
        SERIAL_PORT.write_str("  Allocated: ");
        SERIAL_PORT.write_decimal((allocated / 1024) as u32);
        SERIAL_PORT.write_str(" KB\n");
        SERIAL_PORT.write_str("  Free: ");
        SERIAL_PORT.write_decimal((free / 1024) as u32);
        SERIAL_PORT.write_str(" KB\n");
        SERIAL_PORT.write_str("  Total: ");
        SERIAL_PORT.write_decimal((total / 1024) as u32);
        SERIAL_PORT.write_str(" KB\n");

        for (i, region) in self.regions.iter().enumerate() {
            if let Some(region) = region {
                SERIAL_PORT.write_str("Region ");
                SERIAL_PORT.write_decimal(i as u32);
                SERIAL_PORT.write_str(": 0x");
                SERIAL_PORT.write_hex((region.start >> 32) as u32);
                SERIAL_PORT.write_hex(region.start as u32);
                SERIAL_PORT.write_str(" - 0x");
                SERIAL_PORT.write_hex((region.end >> 32) as u32);
                SERIAL_PORT.write_hex(region.end as u32);
                SERIAL_PORT.write_str(" (used: ");
                SERIAL_PORT.write_decimal((region.used / 1024) as u32);
                SERIAL_PORT.write_str(" KB, available: ");
                SERIAL_PORT.write_decimal((region.available() / 1024) as u32);
                SERIAL_PORT.write_str(" KB)\n");
            }
        }
        SERIAL_PORT.write_str("=======================\n");
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Try to allocate from current region first
        let current_idx = self.current_region.load(Ordering::Relaxed);
        
        // Cast the regions array to *mut to modify it
        let regions_ptr = self.regions.as_ptr() as *mut [Option<MemoryRegion>; 8];
        let regions = &mut *regions_ptr;

        // Try current region
        if let Some(ref mut region) = regions[current_idx] {
            if let Some(ptr) = region.allocate(size, align) {
                self.total_allocated.fetch_add(size, Ordering::Relaxed);
                return ptr.as_ptr();
            }
        }

        // Try other regions
        for (i, region_opt) in regions.iter_mut().enumerate() {
            if i == current_idx {
                continue; // Already tried this one
            }
            
            if let Some(region) = region_opt {
                if let Some(ptr) = region.allocate(size, align) {
                    self.current_region.store(i, Ordering::Relaxed);
                    self.total_allocated.fetch_add(size, Ordering::Relaxed);
                    return ptr.as_ptr();
                }
            }
        }

        // Out of memory
        SERIAL_PORT.write_str("ALLOCATOR: Out of memory! Requested ");
        SERIAL_PORT.write_decimal(size as u32);
        SERIAL_PORT.write_str(" bytes with alignment ");
        SERIAL_PORT.write_decimal(align as u32);
        SERIAL_PORT.write_str("\n");

        ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't support deallocation
        // In a real system, you'd want a more sophisticated allocator
        // that can reclaim memory (like a free list allocator)
    }
}

// ============================================================================
// GLOBAL ALLOCATOR INSTANCE
// ============================================================================

#[global_allocator]
pub static ALLOCATOR: BumpAllocator = BumpAllocator::new();

/// Initialize the global allocator
pub unsafe fn init_heap(memory_map_request: &MemoryMapRequest) {
    // Cast to get mutable access for initialization
    let allocator_ptr = &ALLOCATOR as *const BumpAllocator as *mut BumpAllocator;
    (*allocator_ptr).init(memory_map_request);
}

/// Get allocator statistics
pub fn heap_stats() -> (usize, usize, usize) {
    ALLOCATOR.stats()
}

/// Print allocator debug information
pub unsafe fn debug_heap() {
    ALLOCATOR.debug_info();
}

// ============================================================================
// ALLOCATION ERROR HANDLER
// ============================================================================

#[cfg(feature = "alloc_error_handler")]
#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    unsafe {
        SERIAL_PORT.write_str("ALLOCATION ERROR: Failed to allocate ");
        SERIAL_PORT.write_decimal(layout.size() as u32);
        SERIAL_PORT.write_str(" bytes with alignment ");
        SERIAL_PORT.write_decimal(layout.align() as u32);
        SERIAL_PORT.write_str("\n");
        
        debug_heap();
    }
    
    panic!("Allocation error: out of memory");
}

// Alternative panic-based error handling for stable Rust
#[cfg(not(feature = "alloc_error_handler"))]
fn handle_alloc_error(layout: Layout) -> ! {
    unsafe {
        SERIAL_PORT.write_str("ALLOCATION ERROR: Failed to allocate ");
        SERIAL_PORT.write_decimal(layout.size() as u32);
        SERIAL_PORT.write_str(" bytes with alignment ");
        SERIAL_PORT.write_decimal(layout.align() as u32);
        SERIAL_PORT.write_str("\n");
        
        debug_heap();
    }
    
    panic!("Allocation error: out of memory");
}

// ============================================================================
// CONVENIENCE FUNCTIONS
// ============================================================================

/// Allocate memory for a specific type
pub fn alloc_for_type<T>() -> Option<NonNull<T>> {
    let layout = Layout::new::<T>();
    unsafe {
        let ptr = ALLOCATOR.alloc(layout);
        NonNull::new(ptr as *mut T)
    }
}

/// Allocate memory for an array of a specific type
pub fn alloc_array<T>(count: usize) -> Option<NonNull<T>> {
    if let Ok(layout) = Layout::array::<T>(count) {
        unsafe {
            let ptr = ALLOCATOR.alloc(layout);
            NonNull::new(ptr as *mut T)
        }
    } else {
        None
    }
}