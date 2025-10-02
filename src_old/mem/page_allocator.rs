// src/mem/page_allocator.rs
#![no_std]

use core::cmp;
use spin::Mutex;

/// Simple page allocator used for early boot/testing.
/// This is intentionally small and predictable:
/// - fixed maximum pages (MAX_PAGES)
/// - a simple byte array map: 0 == free, 1 == used
/// - returns physical addresses computed from a base address and page index
///
/// Replace with production allocator later.

pub const PAGE_SIZE: u64 = 4096;
const MAX_PAGES: usize = 16 * 1024; // 16k pages -> ~64MB coverage if 4KiB pages

#[derive(Copy, Clone, Debug)]
pub struct PageAllocatorStats {
    pub total_pages: usize,
    pub free_pages: usize,
}

/// Internal allocator struct
pub struct PageAllocator {
    base: u64,
    total_pages: usize,
    map: [u8; MAX_PAGES], // 0 = free, 1 = used
    initialized: bool,
    free_count: usize,
}

impl PageAllocator {
    pub const fn new() -> Self {
        Self {
            base: 0,
            total_pages: 0,
            map: [1; MAX_PAGES], // default to used until initialized
            initialized: false,
            free_count: 0,
        }
    }

    /// Initialize with base physical address and number of pages available.
    /// Clips total_pages to MAX_PAGES.
    pub fn init(&mut self, base: u64, total_pages: usize) {
        let total = cmp::min(total_pages, MAX_PAGES);
        self.base = base;
        self.total_pages = total;
        self.free_count = total;
        // mark first `total` pages free, rest used
        let mut i = 0usize;
        while i < total {
            self.map[i] = 0;
            i += 1;
        }
        while i < MAX_PAGES {
            self.map[i] = 1;
            i += 1;
        }
        self.initialized = true;
    }

    /// Allocate a single page, returning its physical address.
    pub fn allocate_page(&mut self) -> Option<u64> {
        if !self.initialized {
            return None;
        }
        for i in 0..self.total_pages {
            if self.map[i] == 0 {
                self.map[i] = 1;
                self.free_count = self.free_count.saturating_sub(1);
                return Some(self.base + (i as u64) * PAGE_SIZE);
            }
        }
        None
    }

    /// Allocate `count` contiguous pages and return starting physical address.
    pub fn allocate_pages(&mut self, count: usize) -> Option<u64> {
        if !self.initialized || count == 0 || count > self.total_pages {
            return None;
        }

        let mut run_start: usize = 0;
        let mut run_len: usize = 0;

        for i in 0..self.total_pages {
            if self.map[i] == 0 {
                if run_len == 0 {
                    run_start = i;
                }
                run_len += 1;
                if run_len == count {
                    // mark used
                    for j in run_start..(run_start + count) {
                        self.map[j] = 1;
                    }
                    self.free_count = self.free_count.saturating_sub(count);
                    return Some(self.base + (run_start as u64) * PAGE_SIZE);
                }
            } else {
                run_len = 0;
            }
        }
        None
    }

    /// Free a single page at physical `addr`.
    pub fn free_page(&mut self, addr: u64) -> Result<(), &'static str> {
        if !self.initialized {
            return Err("allocator not initialized");
        }
        if addr < self.base {
            return Err("address below base");
        }
        let idx = ((addr - self.base) / PAGE_SIZE) as usize;
        if idx >= self.total_pages {
            return Err("address out of range");
        }
        if self.map[idx] == 0 {
            // already free
            return Err("page already free");
        }
        self.map[idx] = 0;
        self.free_count = self.free_count.saturating_add(1);
        Ok(())
    }

    /// Free `count` pages starting at physical `addr`.
    pub fn free_pages(&mut self, addr: u64, count: usize) -> Result<(), &'static str> {
        if count == 0 {
            return Err("count is zero");
        }
        if !self.initialized {
            return Err("allocator not initialized");
        }
        if addr < self.base {
            return Err("address below base");
        }
        let start = ((addr - self.base) / PAGE_SIZE) as usize;
        if start + count > self.total_pages {
            return Err("range out of bounds");
        }
        for i in start..(start + count) {
            if self.map[i] == 0 {
                // if already free, continue — we still mark freed pages
            } else {
                self.map[i] = 0;
            }
        }
        // recompute free_count (cheap)
        let mut cnt = 0usize;
        for i in 0..self.total_pages {
            if self.map[i] == 0 {
                cnt += 1;
            }
        }
        self.free_count = cnt;
        Ok(())
    }

    pub fn stats(&self) -> PageAllocatorStats {
        PageAllocatorStats {
            total_pages: self.total_pages,
            free_pages: self.free_count,
        }
    }
}

/// Global allocator protected by a spin lock
pub static PAGE_ALLOCATOR: Mutex<PageAllocator> = Mutex::new(PageAllocator::new());

/// Public wrappers (safe) so other modules do not attempt to take mutable refs to a static.
pub fn init_page_allocator(base: u64, total_pages: usize) {
    let mut g = PAGE_ALLOCATOR.lock();
    g.init(base, total_pages);
}

/// Allocate one page physical address
pub fn allocate_page() -> Option<u64> {
    let mut g = PAGE_ALLOCATOR.lock();
    g.allocate_page()
}

/// Allocate `count` contiguous pages
pub fn allocate_pages(count: usize) -> Option<u64> {
    let mut g = PAGE_ALLOCATOR.lock();
    g.allocate_pages(count)
}

/// Free a single page
pub fn free_page(addr: u64) -> Result<(), &'static str> {
    let mut g = PAGE_ALLOCATOR.lock();
    g.free_page(addr)
}

/// Free count pages
pub fn free_pages(addr: u64, count: usize) -> Result<(), &'static str> {
    let mut g = PAGE_ALLOCATOR.lock();
    g.free_pages(addr, count)
}

/// Get a copy of allocator stats
pub fn page_allocator_stats() -> PageAllocatorStats {
    let g = PAGE_ALLOCATOR.lock();
    g.stats()
}
