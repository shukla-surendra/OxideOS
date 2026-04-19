//! Minimal FAT16 driver for OxideOS.
//!
//! Supports read/write access to a FAT16 volume on the primary ATA IDE disk.
//! The volume is expected to start at LBA 0 (whole-disk FAT16, no partition
//! table). Both FAT copies are kept in sync on writes.
//!
//! # File descriptors
//! Open files occupy FDs 64-79 (16 concurrent open files, subtract 64 to get
//! the internal slot index).
//!
//! # Path format
//! Only root-directory files are supported. Paths may have an optional
//! `/disk/` prefix (e.g., `/disk/README.TXT`) or just a bare `README.TXT`.
//! Names are matched case-insensitively.

extern crate alloc;
use alloc::{string::String, vec::Vec};

use crate::kernel::ata;
use crate::kernel::serial::SERIAL_PORT;

pub const FAT_FD_BASE: i32 = 64;
const FAT_FD_COUNT: usize = 16;

// ── BPB / layout state ─────────────────────────────────────────────────────

struct Bpb {
    bytes_per_sector:   u16,
    sectors_per_cluster: u8,
    reserved_sectors:   u16,
    fat_count:          u8,
    root_entry_count:   u16,
    fat_size_16:        u16,
    fat_start_lba:      u32,
    root_dir_lba:       u32,
    data_start_lba:     u32,
}

// ── Open file descriptor ───────────────────────────────────────────────────

struct FatFd {
    active:        bool,
    writable:      bool,
    file_size:     u32,
    first_cluster: u16,
    cur_cluster:   u16,
    cur_sector:    u8,   // sector within current cluster (0-based)
    file_offset:   u32,  // bytes read/written so far
    /// Index of this entry in the root directory (for updating size on close/write).
    dir_entry_sector: u32,
    dir_entry_offset: u32, // byte offset within that sector (0, 32, 64, …, 480)
}

impl FatFd {
    const fn empty() -> Self {
        Self {
            active:            false,
            writable:          false,
            file_size:         0,
            first_cluster:     0,
            cur_cluster:       0,
            cur_sector:        0,
            file_offset:       0,
            dir_entry_sector:  0,
            dir_entry_offset:  0,
        }
    }
}

// ── Global state ───────────────────────────────────────────────────────────

struct FatFs {
    ready: bool,
    bpb:   Bpb,
    fds:   [FatFd; FAT_FD_COUNT],
}

impl FatFs {
    const fn new() -> Self {
        Self {
            ready: false,
            bpb: Bpb {
                bytes_per_sector:   512,
                sectors_per_cluster: 1,
                reserved_sectors:   1,
                fat_count:          2,
                root_entry_count:   512,
                fat_size_16:        9,
                fat_start_lba:      1,
                root_dir_lba:       19,
                data_start_lba:     51,
            },
            fds: [const { FatFd::empty() }; FAT_FD_COUNT],
        }
    }
}

pub static mut FAT_FS: FatFs = FatFs::new();

// ── Directory location ─────────────────────────────────────────────────────

/// Identifies where a directory lives on the FAT16 volume.
#[derive(Clone, Copy, Debug)]
pub enum DirLoc {
    /// The FAT16 root directory (fixed-size area, not in the data region).
    Root,
    /// A subdirectory stored in the data region; `first_cluster` is its start.
    Subdir(u16),
}

/// Information about a single directory entry.
pub struct DirEntryInfo {
    pub name83:        [u8; 11],
    pub first_cluster: u16,
    pub size:          u32,
    /// LBA of the 512-byte sector that contains this entry.
    pub entry_sector:  u32,
    /// Byte offset of the entry within that sector (multiple of 32).
    pub entry_offset:  u32,
    pub is_dir:        bool,
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Sector offset of a cluster's first sector in the data area.
fn cluster_to_lba(bpb: &Bpb, cluster: u16) -> u32 {
    bpb.data_start_lba + (cluster as u32 - 2) * bpb.sectors_per_cluster as u32
}

/// Iterate all non-deleted, non-volume-label entries in a directory.
/// Calls `f(&entry) -> bool`; returning `false` stops iteration.
unsafe fn for_each_fat_entry(
    bpb: &Bpb,
    dir: DirLoc,
    mut f: impl FnMut(&DirEntryInfo) -> bool,
) {
    let mut sector_buf = [0u8; 512];

    match dir {
        DirLoc::Root => {
            let root_sectors = (bpb.root_entry_count as u32 * 32 + 511) / 512;
            'outer: for s in 0..root_sectors {
                let lba = bpb.root_dir_lba + s;
                if !unsafe { read_sector_buf(lba, &mut sector_buf) } { break; }
                for e in 0..16u32 {
                    let off = (e * 32) as usize;
                    if sector_buf[off] == 0x00 { break 'outer; }
                    if sector_buf[off] == 0xE5 { continue; }
                    let attr = sector_buf[off + 11];
                    if attr & 0x08 != 0 { continue; } // volume label
                    let mut n83 = [0u8; 11];
                    n83.copy_from_slice(&sector_buf[off..off + 11]);
                    let entry = DirEntryInfo {
                        name83:        n83,
                        first_cluster: u16::from_le_bytes([sector_buf[off+26], sector_buf[off+27]]),
                        size:          u32::from_le_bytes([sector_buf[off+28], sector_buf[off+29], sector_buf[off+30], sector_buf[off+31]]),
                        entry_sector:  lba,
                        entry_offset:  e * 32,
                        is_dir:        attr & 0x10 != 0,
                    };
                    if !f(&entry) { return; }
                }
            }
        }

        DirLoc::Subdir(first_cluster) => {
            let mut cluster = first_cluster;
            'chain: loop {
                if cluster < 2 || cluster >= 0xFFF8 { break; }
                let cluster_lba = cluster_to_lba(bpb, cluster);
                for s in 0..bpb.sectors_per_cluster as u32 {
                    let lba = cluster_lba + s;
                    if !unsafe { read_sector_buf(lba, &mut sector_buf) } { break 'chain; }
                    for e in 0..16u32 {
                        let off = (e * 32) as usize;
                        if sector_buf[off] == 0x00 { return; }
                        if sector_buf[off] == 0xE5 { continue; }
                        let attr = sector_buf[off + 11];
                        if attr & 0x08 != 0 { continue; } // volume label
                        let mut n83 = [0u8; 11];
                        n83.copy_from_slice(&sector_buf[off..off + 11]);
                        let entry = DirEntryInfo {
                            name83:        n83,
                            first_cluster: u16::from_le_bytes([sector_buf[off+26], sector_buf[off+27]]),
                            size:          u32::from_le_bytes([sector_buf[off+28], sector_buf[off+29], sector_buf[off+30], sector_buf[off+31]]),
                            entry_sector:  lba,
                            entry_offset:  e * 32,
                            is_dir:        attr & 0x10 != 0,
                        };
                        if !f(&entry) { return; }
                    }
                }
                cluster = unsafe { fat_next(bpb, cluster) };
            }
        }
    }
}

/// Strip the `/disk/` or `/disk` prefix from a FAT path, returning the
/// volume-relative part (may be empty for the root, e.g. `/disk`).
fn strip_disk_prefix(raw: &[u8]) -> &[u8] {
    if raw.starts_with(b"/disk/") { &raw[6..] }
    else if raw == b"/disk"       { b"" }
    else if raw.starts_with(b"/") { &raw[1..] }
    else                          { raw }
}

/// Convert a single path component (no slashes) to an 8.3 directory-name.
fn component_to_83(part: &[u8]) -> Option<[u8; 11]> {
    if part.is_empty() { return None; }
    let (name_part, ext_part) = match part.iter().position(|&b| b == b'.') {
        Some(dot) => (&part[..dot], &part[dot + 1..]),
        None      => (part, &b""[..]),
    };
    if name_part.is_empty() { return None; }
    let mut n83 = [b' '; 11];
    for (i, &b) in name_part.iter().take(8).enumerate() {
        n83[i] = b.to_ascii_uppercase();
    }
    for (i, &b) in ext_part.iter().take(3).enumerate() {
        n83[8 + i] = b.to_ascii_uppercase();
    }
    Some(n83)
}

/// Resolve a sequence of path components starting from `start_dir`,
/// returning the `DirLoc` of the final directory component, or `None`
/// if any component is not a directory that exists.
unsafe fn resolve_dir_components(bpb: &Bpb, start_dir: DirLoc, components: &[u8]) -> Option<DirLoc> {
    if components.is_empty() { return Some(start_dir); }
    let mut current = start_dir;
    for part in components.split(|&b| b == b'/') {
        if part.is_empty() { continue; }
        if part == b"." { continue; }
        if part == b".." {
            // For simplicity, `..` from root stays at root.
            // Full `..` traversal would require tracking parent clusters.
            if let DirLoc::Root = current { continue; }
            // Can't go up from a subdir without a parent tracker — not supported yet.
            return None;
        }
        let n83 = component_to_83(part)?;
        let mut found: Option<DirLoc> = None;
        unsafe {
            for_each_fat_entry(bpb, current, |e| {
                if e.name83 == n83 {
                    if e.is_dir {
                        found = Some(DirLoc::Subdir(e.first_cluster));
                    }
                    return false; // stop
                }
                true
            });
        }
        current = found?;
    }
    Some(current)
}

/// Resolve a full FAT path to `(parent_dir, filename_83)`.
/// `raw_path` is the raw bytes as supplied to `open`/`mkdir`/etc.
/// Returns `None` for invalid paths.
unsafe fn resolve_parent(bpb: &Bpb, raw_path: &[u8]) -> Option<(DirLoc, [u8; 11])> {
    let rel = strip_disk_prefix(raw_path);
    if rel.is_empty() { return None; }

    // Split at the last `/` to get (dir_part, file_name).
    let (dir_bytes, file_bytes) = match rel.iter().rposition(|&b| b == b'/') {
        Some(pos) => (&rel[..pos], &rel[pos + 1..]),
        None      => (&b""[..], rel),
    };

    let parent = unsafe { resolve_dir_components(bpb, DirLoc::Root, dir_bytes)? };
    let name83 = component_to_83(file_bytes)?;
    Some((parent, name83))
}

/// Read one 512-byte sector into a stack buffer. Returns false on error.
unsafe fn read_sector_buf(lba: u32, buf: &mut [u8; 512]) -> bool {
    unsafe { ata::read_sector(lba, buf) }
}

/// Write one 512-byte sector from a stack buffer. Returns false on error.
unsafe fn write_sector_buf(lba: u32, buf: &[u8; 512]) -> bool {
    unsafe { ata::write_sector(lba, buf) }
}

/// Write a FAT16 entry for `cluster` to both FAT copies.
unsafe fn fat_write_entry(bpb: &Bpb, cluster: u16, value: u16) -> bool {
    let entry_offset    = cluster as u32 * 2;
    let sector_in_fat   = entry_offset / 512;
    let byte_in_sector  = (entry_offset % 512) as usize;

    let fat_lba = bpb.fat_start_lba + sector_in_fat;
    let mut buf = [0u8; 512];

    // Read-modify-write for each FAT copy.
    for copy in 0..bpb.fat_count as u32 {
        let lba = fat_lba + copy * bpb.fat_size_16 as u32;
        if !unsafe { read_sector_buf(lba, &mut buf) } { return false; }
        buf[byte_in_sector]     = (value & 0xFF) as u8;
        buf[byte_in_sector + 1] = (value >> 8)   as u8;
        if !unsafe { write_sector_buf(lba, &buf) } { return false; }
    }
    true
}

/// Allocate a free cluster (value 0x0000 in FAT), mark it end-of-chain (0xFFFF).
/// Returns the cluster number, or 0 on failure.
unsafe fn fat_alloc_cluster(bpb: &Bpb) -> u16 {
    let mut buf = [0u8; 512];
    // Scan every FAT sector for a free entry (cluster 2 onwards).
    for s in 0..bpb.fat_size_16 as u32 {
        let lba = bpb.fat_start_lba + s;
        if !unsafe { read_sector_buf(lba, &mut buf) } { continue; }
        for i in (0..512usize).step_by(2) {
            let lo = buf[i] as u16;
            let hi = buf[i + 1] as u16;
            let cluster = (s * 256 + (i / 2) as u32) as u16;
            if cluster < 2 { continue; }
            if lo == 0 && hi == 0 {
                // Found a free cluster — mark end-of-chain.
                if unsafe { fat_write_entry(bpb, cluster, 0xFFFF) } {
                    return cluster;
                }
                return 0;
            }
        }
    }
    0 // disk full
}

/// Follow the FAT chain for `cluster`, returning the next cluster number.
/// Returns 0xFFFF when at end-of-chain, 0 on error.
unsafe fn fat_next(bpb: &Bpb, cluster: u16) -> u16 {
    // Each FAT16 entry is 2 bytes. Figure out which sector holds it.
    let entry_offset = cluster as u32 * 2;
    let sector_in_fat = entry_offset / 512;
    let byte_in_sector = (entry_offset % 512) as usize;

    let fat_lba = bpb.fat_start_lba + sector_in_fat;
    let mut buf = [0u8; 512];
    if !unsafe { read_sector_buf(fat_lba, &mut buf) } { return 0; }

    let lo = buf[byte_in_sector] as u16;
    let hi = buf[byte_in_sector + 1] as u16;
    let val = lo | (hi << 8);
    if val >= 0xFFF8 { 0xFFFF } else { val }
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Initialise the FAT16 driver.
///
/// Must be called after `ata::init()` and `mbr::init()`.  Supports both
/// whole-disk FAT16 (legacy oxide_disk.img) and a FAT16 partition on a
/// partitioned install image — the partition offset is read from the MBR.
pub unsafe fn init() {
    if !ata::is_present() {
        unsafe { SERIAL_PORT.write_str("FAT16: no ATA disk, skipping\n"); }
        return;
    }

    // Whole-disk FAT16 → part_offset = 0; partitioned disk → MBR entry start LBA.
    let part_offset = unsafe { crate::kernel::mbr::fat16_lba_offset() };

    let mut buf = [0u8; 512];
    if !unsafe { read_sector_buf(part_offset, &mut buf) } {
        unsafe { SERIAL_PORT.write_str("FAT16: failed to read boot sector\n"); }
        return;
    }

    if buf[510] != 0x55 || buf[511] != 0xAA {
        unsafe { SERIAL_PORT.write_str("FAT16: bad boot sector signature\n"); }
        return;
    }

    let fs = &raw mut FAT_FS;

    (*fs).bpb.bytes_per_sector    = u16::from_le_bytes([buf[0x0B], buf[0x0C]]);
    (*fs).bpb.sectors_per_cluster = buf[0x0D];
    (*fs).bpb.reserved_sectors    = u16::from_le_bytes([buf[0x0E], buf[0x0F]]);
    (*fs).bpb.fat_count           = buf[0x10];
    (*fs).bpb.root_entry_count    = u16::from_le_bytes([buf[0x11], buf[0x12]]);
    (*fs).bpb.fat_size_16         = u16::from_le_bytes([buf[0x16], buf[0x17]]);

    let reserved  = (*fs).bpb.reserved_sectors as u32;
    let fat_count = (*fs).bpb.fat_count as u32;
    let fat_size  = (*fs).bpb.fat_size_16 as u32;
    let rde       = (*fs).bpb.root_entry_count as u32;

    // All stored LBAs are absolute disk LBAs (partition offset already baked in).
    (*fs).bpb.fat_start_lba  = part_offset + reserved;
    (*fs).bpb.root_dir_lba   = part_offset + reserved + fat_count * fat_size;
    (*fs).bpb.data_start_lba = (*fs).bpb.root_dir_lba + (rde * 32 + 511) / 512;

    (*fs).ready = true;

    unsafe {
        SERIAL_PORT.write_str("FAT16: mounted at part_lba=");
        SERIAL_PORT.write_decimal(part_offset);
        SERIAL_PORT.write_str(" root_dir_lba=");
        SERIAL_PORT.write_decimal((*fs).bpb.root_dir_lba);
        SERIAL_PORT.write_str(" data_start_lba=");
        SERIAL_PORT.write_decimal((*fs).bpb.data_start_lba);
        SERIAL_PORT.write_str("\n");
    }
}

/// Returns `true` if `fd` is in the FAT FD range.
pub fn is_fat_fd(fd: i32) -> bool {
    fd >= FAT_FD_BASE && fd < FAT_FD_BASE + FAT_FD_COUNT as i32
}

/// Seek within an open FAT16 file.
/// `whence`: 0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END.
/// Returns new file offset (≥0) or -22 (EINVAL) on error.
pub fn file_seek(fd: i32, offset: i64, whence: u32) -> i64 {
    if !is_fat_fd(fd) { return -9; } // EBADF
    let slot = (fd - FAT_FD_BASE) as usize;
    unsafe {
        let fs = &raw mut FAT_FS;
        let fsize = (*fs).fds[slot].file_size as i64;
        let cur   = (*fs).fds[slot].file_offset as i64;
        let new_off = match whence {
            0 => offset,          // SEEK_SET
            1 => cur + offset,    // SEEK_CUR
            2 => fsize + offset,  // SEEK_END
            _ => return -22,
        };
        if new_off < 0 { return -22; }
        (*fs).fds[slot].file_offset = new_off as u32;
        new_off
    }
}

/// Return the size (in bytes) of the open file `fd`.  Returns 0 if `fd` is invalid.
pub fn file_size(fd: i32) -> u32 {
    if !is_fat_fd(fd) { return 0; }
    let slot = (fd - FAT_FD_BASE) as usize;
    unsafe {
        let fs = &raw const FAT_FS;
        (*fs).fds[slot].file_size
    }
}

/// Open a file by path. Returns an FD ≥ 64 on success, negative on error.
/// `flags` bits: O_RDONLY=0, O_WRONLY=1, O_RDWR=2, O_CREAT=0x40, O_TRUNC=0x200.
/// Supports subdirectory paths (e.g. `/disk/bin/sh`).
pub unsafe fn open(raw_path: &[u8], flags: u32) -> i64 {
    let fs = &raw mut FAT_FS;
    if !(*fs).ready { return -2; }

    let (parent_dir, dir_name) = match unsafe { resolve_parent(&(*fs).bpb, raw_path) } {
        Some(r) => r,
        None    => return -22, // EINVAL
    };

    let writable  = (flags & crate::kernel::fs::O_WRONLY != 0)
                 || (flags & crate::kernel::fs::O_RDWR   != 0);
    let do_create = flags & crate::kernel::fs::O_CREAT  != 0;
    let do_trunc  = flags & crate::kernel::fs::O_TRUNC  != 0;

    // Search parent directory for an existing file entry.
    let mut found_sector: u32  = 0;
    let mut found_off:    u32  = 0;
    let mut found_fc:     u16  = 0;
    let mut found_size:   u32  = 0;
    let mut found: bool = false;

    unsafe {
        for_each_fat_entry(&(*fs).bpb, parent_dir, |e| {
            if e.name83 == dir_name && !e.is_dir {
                found_sector = e.entry_sector;
                found_off    = e.entry_offset;
                found_fc     = e.first_cluster;
                found_size   = e.size;
                found = true;
                return false; // stop
            }
            true
        });
    }

    if !found {
        if !do_create { return -7; } // ENOENT
        // Create a new entry only in root (subdir creation for O_CREAT in subdir
        // requires finding a free slot in that dir's cluster chain — full impl below).
        let new_cluster = unsafe { fat_alloc_cluster(&(*fs).bpb) };
        if new_cluster == 0 { return -28; } // ENOSPC

        let created = unsafe { create_dir_entry(&mut (*fs).bpb, parent_dir, &dir_name, 0x20, new_cluster, 0,
                                                 &mut found_sector, &mut found_off) };
        if !created { return -28; }
        found_fc   = new_cluster;
        found_size = 0;
        found = true;
    }

    // Truncate: free cluster chain and reset size.
    if do_trunc && writable && found_fc != 0 {
        let mut cl = found_fc;
        while cl >= 2 && cl < 0xFFF8 {
            let next = unsafe { fat_next(&(*fs).bpb, cl) };
            let _ = unsafe { fat_write_entry(&(*fs).bpb, cl, 0x0000) };
            cl = next;
        }
        let new_cl = unsafe { fat_alloc_cluster(&(*fs).bpb) };
        found_fc   = new_cl;
        found_size = 0;
        let mut buf = [0u8; 512];
        if unsafe { read_sector_buf(found_sector, &mut buf) } {
            let off = found_off as usize;
            buf[off + 26] = (new_cl & 0xFF) as u8;
            buf[off + 27] = (new_cl >> 8)   as u8;
            buf[off + 28..off + 32].fill(0);
            let _ = unsafe { write_sector_buf(found_sector, &buf) };
        }
    }

    // Allocate FD slot.
    let fds = &raw mut (*fs).fds;
    for i in 0..FAT_FD_COUNT {
        let fd_slot = &raw mut (*fds)[i];
        if !(*fd_slot).active {
            (*fd_slot).active           = true;
            (*fd_slot).writable         = writable;
            (*fd_slot).file_size        = found_size;
            (*fd_slot).first_cluster    = found_fc;
            (*fd_slot).cur_cluster      = found_fc;
            (*fd_slot).cur_sector       = 0;
            (*fd_slot).file_offset      = 0;
            (*fd_slot).dir_entry_sector = found_sector;
            (*fd_slot).dir_entry_offset = found_off;
            return (FAT_FD_BASE + i as i32) as i64;
        }
    }
    -4 // ENOMEM: no free FD slots
}

/// Write a new 32-byte entry into a directory (root or subdir).
/// `attr` is the FAT attribute byte (0x20 = file, 0x10 = dir).
/// Writes back the sector/offset of the new entry into `sector_out`/`off_out`.
/// Returns `true` on success.
unsafe fn create_dir_entry(
    bpb: &Bpb,
    dir: DirLoc,
    name83: &[u8; 11],
    attr: u8,
    first_cluster: u16,
    size: u32,
    sector_out: &mut u32,
    off_out: &mut u32,
) -> bool {
    let mut buf = [0u8; 512];

    let try_sector = |lba: u32, buf: &mut [u8; 512], sector_out: &mut u32, off_out: &mut u32| -> bool {
        if !unsafe { read_sector_buf(lba, buf) } { return false; }
        for e in 0..16u32 {
            let off = (e * 32) as usize;
            if buf[off] == 0x00 || buf[off] == 0xE5 {
                buf[off..off + 11].copy_from_slice(name83);
                buf[off + 11] = attr;
                buf[off + 12..off + 26].fill(0);
                buf[off + 26] = (first_cluster & 0xFF) as u8;
                buf[off + 27] = (first_cluster >> 8)   as u8;
                buf[off + 28] = (size       & 0xFF) as u8;
                buf[off + 29] = ((size >>  8) & 0xFF) as u8;
                buf[off + 30] = ((size >> 16) & 0xFF) as u8;
                buf[off + 31] = ((size >> 24) & 0xFF) as u8;
                if !unsafe { write_sector_buf(lba, buf) } { return false; }
                *sector_out = lba;
                *off_out    = e * 32;
                return true;
            }
        }
        false
    };

    match dir {
        DirLoc::Root => {
            let root_sectors = (bpb.root_entry_count as u32 * 32 + 511) / 512;
            for s in 0..root_sectors {
                let lba = bpb.root_dir_lba + s;
                if try_sector(lba, &mut buf, sector_out, off_out) { return true; }
            }
            false
        }
        DirLoc::Subdir(first_cl) => {
            let mut cluster = first_cl;
            loop {
                if cluster < 2 || cluster >= 0xFFF8 { break; }
                let cluster_lba = cluster_to_lba(bpb, cluster);
                for s in 0..bpb.sectors_per_cluster as u32 {
                    let lba = cluster_lba + s;
                    if try_sector(lba, &mut buf, sector_out, off_out) { return true; }
                }
                let next = unsafe { fat_next(bpb, cluster) };
                if next >= 0xFFF8 || next == 0 {
                    // Extend the directory with a new cluster.
                    let new_cl = unsafe { fat_alloc_cluster(bpb) };
                    if new_cl == 0 { return false; }
                    let _ = unsafe { fat_write_entry(bpb, cluster, new_cl) };
                    // Zero the new cluster so all entries start as 0x00.
                    let new_lba = cluster_to_lba(bpb, new_cl);
                    let zero = [0u8; 512];
                    for s in 0..bpb.sectors_per_cluster as u32 {
                        let _ = unsafe { write_sector_buf(new_lba + s, &zero) };
                    }
                    cluster = new_cl;
                } else {
                    cluster = next;
                }
            }
            false
        }
    }
}

/// Check whether a path points to an existing directory on FAT.
/// Returns `true` if the path resolves to a dir (or root `/disk`).
pub unsafe fn is_directory(raw_path: &[u8]) -> bool {
    let fs = &raw const FAT_FS;
    if !(*fs).ready { return false; }

    let rel = strip_disk_prefix(raw_path);
    if rel.is_empty() { return true; } // `/disk` itself is the root

    // Split into parent dir + final component.
    let (dir_bytes, file_bytes) = match rel.iter().rposition(|&b| b == b'/') {
        Some(pos) => (&rel[..pos], &rel[pos + 1..]),
        None      => (&b""[..], rel),
    };

    let parent = match unsafe { resolve_dir_components(&(*fs).bpb, DirLoc::Root, dir_bytes) } {
        Some(d) => d,
        None    => return false,
    };
    let name83 = match component_to_83(file_bytes) {
        Some(n) => n,
        None    => return false,
    };

    let mut found = false;
    unsafe {
        for_each_fat_entry(&(*fs).bpb, parent, |e| {
            if e.name83 == name83 && e.is_dir {
                found = true;
                return false;
            }
            true
        });
    }
    found
}

/// Create a new subdirectory at `raw_path`.
/// Returns 0 on success, negative on error.
pub unsafe fn mkdir(raw_path: &[u8]) -> i64 {
    let fs = &raw mut FAT_FS;
    if !(*fs).ready { return -2; }

    let (parent_dir, dir_name) = match unsafe { resolve_parent(&(*fs).bpb, raw_path) } {
        Some(r) => r,
        None    => return -22,
    };

    // Allocate a cluster for the new directory's data.
    let new_cluster = unsafe { fat_alloc_cluster(&(*fs).bpb) };
    if new_cluster == 0 { return -28; }

    // Zero the new cluster.
    let new_lba = cluster_to_lba(&(*fs).bpb, new_cluster);
    let zero = [0u8; 512];
    for s in 0..(*fs).bpb.sectors_per_cluster as u32 {
        if !unsafe { write_sector_buf(new_lba + s, &zero) } { return -5; }
    }

    // Write `.` and `..` entries in the first sector of the new cluster.
    let mut buf = [0u8; 512];
    if !unsafe { read_sector_buf(new_lba, &mut buf) } { return -5; }

    // `.` entry (points to self)
    let dot: [u8; 11] = *b".          ";
    buf[0..11].copy_from_slice(&dot);
    buf[11] = 0x10; // ATTR_DIRECTORY
    buf[12..26].fill(0);
    buf[26] = (new_cluster & 0xFF) as u8;
    buf[27] = (new_cluster >> 8)   as u8;
    buf[28..32].fill(0);

    // `..` entry (points to parent; 0 = root)
    let dotdot: [u8; 11] = *b"..         ";
    buf[32..43].copy_from_slice(&dotdot);
    buf[43] = 0x10;
    buf[44..58].fill(0);
    let parent_cluster: u16 = match parent_dir { DirLoc::Subdir(cl) => cl, DirLoc::Root => 0 };
    buf[58] = (parent_cluster & 0xFF) as u8;
    buf[59] = (parent_cluster >> 8)   as u8;
    buf[60..64].fill(0);

    if !unsafe { write_sector_buf(new_lba, &buf) } { return -5; }

    // Create an entry for the new dir in the parent directory.
    let mut ent_sector = 0u32;
    let mut ent_off    = 0u32;
    if !unsafe { create_dir_entry(&(*fs).bpb, parent_dir, &dir_name, 0x10,
                                   new_cluster, 0, &mut ent_sector, &mut ent_off) } {
        return -28;
    }
    0
}

/// Flush the file size stored in the root-directory entry for `slot`.
unsafe fn flush_dir_size(fs: *mut FatFs, slot: *mut FatFd) {
    let lba = (*slot).dir_entry_sector;
    let off = (*slot).dir_entry_offset as usize;
    if lba == 0 { return; }
    let mut buf = [0u8; 512];
    if !unsafe { read_sector_buf(lba, &mut buf) } { return; }
    let size = (*slot).file_size;
    buf[off + 28] = (size       & 0xFF) as u8;
    buf[off + 29] = ((size >>  8) & 0xFF) as u8;
    buf[off + 30] = ((size >> 16) & 0xFF) as u8;
    buf[off + 31] = ((size >> 24) & 0xFF) as u8;
    let fc = (*slot).first_cluster;
    buf[off + 26] = (fc & 0xFF) as u8;
    buf[off + 27] = (fc >> 8) as u8;
    let _ = unsafe { write_sector_buf(lba, &buf) };
    let _ = fs; // suppress unused warning
}

/// Read up to `buf.len()` bytes from an open FD. Returns bytes read.
pub unsafe fn read_fd(fd: i32, buf: &mut [u8]) -> i64 {
    let fs = &raw mut FAT_FS;
    if !(*fs).ready || !is_fat_fd(fd) { return -5; }

    let idx = (fd - FAT_FD_BASE) as usize;
    let fds = &raw mut (*fs).fds;
    let slot = &raw mut (*fds)[idx];
    if !(*slot).active { return -5; }

    let remaining_in_file = (*slot).file_size.saturating_sub((*slot).file_offset) as usize;
    if remaining_in_file == 0 { return 0; }

    let to_read = buf.len().min(remaining_in_file);
    let mut done = 0usize;
    let bpb_ptr = &raw const (*fs).bpb;

    while done < to_read {
        // Compute LBA for current position
        let spc = (*bpb_ptr).sectors_per_cluster as u32;
        let cluster_lba = cluster_to_lba(&*bpb_ptr, (*slot).cur_cluster);
        let lba = cluster_lba + (*slot).cur_sector as u32;

        let mut sector_buf = [0u8; 512];
        if !unsafe { read_sector_buf(lba, &mut sector_buf) } { break; }

        let byte_off = (*slot).file_offset as usize % 512;
        let avail = (512 - byte_off).min(to_read - done);

        for i in 0..avail {
            buf[done + i] = sector_buf[byte_off + i];
        }
        done += avail;
        (*slot).file_offset += avail as u32;

        // Advance sector/cluster pointer
        if (*slot).file_offset % 512 == 0 {
            (*slot).cur_sector += 1;
            if (*slot).cur_sector >= spc as u8 {
                (*slot).cur_sector = 0;
                let next = unsafe { fat_next(&*bpb_ptr, (*slot).cur_cluster) };
                if next == 0xFFFF || next == 0 { break; }
                (*slot).cur_cluster = next;
            }
        }
    }

    done as i64
}

/// Write up to `buf.len()` bytes to an open, writable FD.  Allocates new
/// clusters automatically as the file grows.  Returns bytes written or negative.
pub unsafe fn write_fd(fd: i32, buf: &[u8]) -> i64 {
    let fs = &raw mut FAT_FS;
    if !(*fs).ready || !is_fat_fd(fd) { return -5; }
    let idx  = (fd - FAT_FD_BASE) as usize;
    let fds  = &raw mut (*fs).fds;
    let slot = &raw mut (*fds)[idx];
    if !(*slot).active  { return -5; }
    if !(*slot).writable { return -9; } // EBADF (read-only)

    let bpb_ptr = &raw const (*fs).bpb;
    let spc     = (*bpb_ptr).sectors_per_cluster as u32;
    let mut done = 0usize;

    while done < buf.len() {
        // If the file has no cluster yet (e.g. newly created with size 0 and
        // alloc failed earlier), allocate one now.
        if (*slot).cur_cluster < 2 {
            let cl = unsafe { fat_alloc_cluster(&*bpb_ptr) };
            if cl == 0 { break; }
            (*slot).first_cluster = cl;
            (*slot).cur_cluster   = cl;
            (*slot).cur_sector    = 0;
        }

        let cluster_lba = cluster_to_lba(&*bpb_ptr, (*slot).cur_cluster);
        let lba         = cluster_lba + (*slot).cur_sector as u32;

        let byte_off = (*slot).file_offset as usize % 512;
        let avail    = (512 - byte_off).min(buf.len() - done);

        // Read-modify-write (unless we're writing a full sector).
        let mut sector_buf = [0u8; 512];
        if byte_off != 0 || avail < 512 {
            if !unsafe { read_sector_buf(lba, &mut sector_buf) } { break; }
        }
        sector_buf[byte_off..byte_off + avail].copy_from_slice(&buf[done..done + avail]);
        if !unsafe { write_sector_buf(lba, &sector_buf) } { break; }

        done                    += avail;
        (*slot).file_offset     += avail as u32;
        if (*slot).file_offset > (*slot).file_size {
            (*slot).file_size = (*slot).file_offset;
        }

        // Advance sector / cluster pointer.
        if (*slot).file_offset % 512 == 0 {
            (*slot).cur_sector += 1;
            if (*slot).cur_sector >= spc as u8 {
                (*slot).cur_sector = 0;
                let next = unsafe { fat_next(&*bpb_ptr, (*slot).cur_cluster) };
                if next >= 0xFFF8 || next == 0 {
                    // Need a new cluster.
                    if done < buf.len() {
                        let new_cl = unsafe { fat_alloc_cluster(&*bpb_ptr) };
                        if new_cl == 0 { break; }
                        // Link current cluster → new.
                        let _ = unsafe { fat_write_entry(&*bpb_ptr, (*slot).cur_cluster, new_cl) };
                        (*slot).cur_cluster = new_cl;
                    }
                } else {
                    (*slot).cur_cluster = next;
                }
            }
        }
    }

    // Update directory entry size after each write.
    let _ = unsafe { flush_dir_size(fs, slot) };

    done as i64
}

/// Close an open FAT FD (flush size to directory if writable).
pub unsafe fn close(fd: i32) -> i64 {
    let fs = &raw mut FAT_FS;
    if !is_fat_fd(fd) { return -5; }
    let idx = (fd - FAT_FD_BASE) as usize;
    let fds = &raw mut (*fs).fds;
    let slot = &raw mut (*fds)[idx];
    if (*slot).active && (*slot).writable {
        let _ = unsafe { flush_dir_size(fs, slot) };
    }
    (*slot).active = false;
    0
}

/// Convert an 8.3 FAT name pair (name bytes, ext bytes) into a lowercase String.
fn fat83_to_string(name: &[u8], ext: &[u8]) -> String {
    let mut s = String::new();
    for &b in name {
        if b == b' ' { break; }
        s.push(b.to_ascii_lowercase() as char);
    }
    let mut ext_len = ext.len();
    while ext_len > 0 && ext[ext_len - 1] == b' ' { ext_len -= 1; }
    if ext_len > 0 {
        s.push('.');
        for &b in &ext[..ext_len] {
            s.push(b.to_ascii_lowercase() as char);
        }
    }
    s
}

/// Resolve the `DirLoc` for a given raw path (e.g. `/disk/bin` → Subdir(cluster)).
/// `/disk` or empty relative path → `DirLoc::Root`.
pub unsafe fn resolve_dir(raw_path: &[u8]) -> Option<DirLoc> {
    let fs = &raw const FAT_FS;
    if !(*fs).ready { return None; }
    let rel = strip_disk_prefix(raw_path);
    unsafe { resolve_dir_components(&(*fs).bpb, DirLoc::Root, rel) }
}

/// Write directory entries for `dir` into `out` as `<name>\n` lines
/// (directories get a trailing `/`).  Returns bytes written or -2 if not ready.
pub unsafe fn list_dir_raw(dir: DirLoc, out: &mut [u8]) -> i64 {
    let fs = &raw const FAT_FS;
    if !(*fs).ready { return -2; }

    let mut written = 0usize;
    unsafe {
        for_each_fat_entry(&(*fs).bpb, dir, |e| {
            let name = fat83_to_string(&e.name83[..8], &e.name83[8..11]);
            if name == "." || name == ".." { return true; }
            let bytes = name.as_bytes();
            let suffix: &[u8] = if e.is_dir { b"/" } else { b"" };
            let needed = bytes.len() + suffix.len() + 1; // +1 for '\n'
            if written + needed > out.len() { return false; }
            out[written..written + bytes.len()].copy_from_slice(bytes);
            written += bytes.len();
            out[written..written + suffix.len()].copy_from_slice(suffix);
            written += suffix.len();
            out[written] = b'\n';
            written += 1;
            true
        });
    }
    written as i64
}

/// Convenience: list the FAT root directory into a raw byte buffer.
/// Kept for backward compatibility with callers that use the old API.
pub unsafe fn list_root_raw(out: &mut [u8]) -> i64 {
    unsafe { list_dir_raw(DirLoc::Root, out) }
}

/// List entries in `dir` as `Vec<(name, is_dir)>`.
pub unsafe fn list_dir(dir: DirLoc) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    let fs = &raw const FAT_FS;
    if !(*fs).ready { return entries; }

    unsafe {
        for_each_fat_entry(&(*fs).bpb, dir, |e| {
            let name = fat83_to_string(&e.name83[..8], &e.name83[8..11]);
            if name != "." && name != ".." {
                entries.push((name, e.is_dir));
            }
            true
        });
    }
    entries
}

/// Convenience: list the FAT root directory.
/// Kept for backward compatibility.
pub unsafe fn list_root() -> Vec<(String, bool)> {
    unsafe { list_dir(DirLoc::Root) }
}
