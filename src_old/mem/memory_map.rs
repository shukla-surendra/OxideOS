// src/mem/memory_map.rs
#![no_std]

use core::fmt;
use spin::Mutex;

/// Small fixed-size memory map useful for early boot.
/// This is intentionally simple and no-alloc to fit  `no_std` kernels.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MemoryRegionType {
    Usable = 1,
    Reserved = 2,
    Acpi = 3,
    Nvs = 4,
    Bad = 5,
}

#[derive(Copy, Clone, Debug)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub region_type: MemoryRegionType,
}

impl MemoryRegion {
    pub const fn new(base: u64, length: u64, region_type: MemoryRegionType) -> Self {
        Self {
            base,
            length,
            region_type,
        }
    }
}

/// Fixed-capacity memory map (no heap).
pub struct MemoryMap {
    regions: [Option<MemoryRegion>; Self::MAX_REGIONS],
    count: usize,
}

impl MemoryMap {
    pub const MAX_REGIONS: usize = 32;

    pub const fn new() -> Self {
        const NONE: Option<MemoryRegion> = None;
        Self {
            regions: [NONE; Self::MAX_REGIONS],
            count: 0,
        }
    }

    /// Add a region (returns Err if full)
    pub fn add_region(&mut self, r: MemoryRegion) -> Result<(), &'static str> {
        if self.count >= Self::MAX_REGIONS {
            return Err("memory map full");
        }
        self.regions[self.count] = Some(r);
        self.count += 1;
        Ok(())
    }

    /// Number of regions currently stored
    pub fn len(&self) -> usize {
        self.count
    }

    /// Immutable iterator over present regions
    pub fn iter(&self) -> MemoryMapIter<'_> {
        MemoryMapIter { map: self, idx: 0 }
    }
}

pub struct MemoryMapIter<'a> {
    map: &'a MemoryMap,
    idx: usize,
}

impl<'a> Iterator for MemoryMapIter<'a> {
    type Item = &'a MemoryRegion;

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.map.count {
            if let Some(ref r) = self.map.regions[self.idx] {
                self.idx += 1;
                return Some(r);
            }
            self.idx += 1;
        }
        None
    }
}

/// The global memory map — protected by a spin::Mutex for `no_std` kernels.
pub static MEMORY_MAP: Mutex<MemoryMap> = Mutex::new(MemoryMap::new());

/// Return the global mutex (callers should `.lock()` it).
pub fn get_memory_map() -> &'static Mutex<MemoryMap> {
    &MEMORY_MAP
}
