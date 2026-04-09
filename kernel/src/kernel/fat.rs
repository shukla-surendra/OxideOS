//! Minimal FAT16 driver for OxideOS.
//!
//! Supports read-only access to a FAT16 volume on the primary ATA IDE disk.
//! The volume is expected to start at LBA 0 (whole-disk FAT16, no partition
//! table). Write support is not yet implemented.
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
    active:       bool,
    file_size:    u32,
    first_cluster: u16,
    cur_cluster:  u16,
    cur_sector:   u8,   // sector within current cluster (0-based)
    file_offset:  u32,  // bytes read so far
}

impl FatFd {
    const fn empty() -> Self {
        Self {
            active:        false,
            file_size:     0,
            first_cluster: 0,
            cur_cluster:   0,
            cur_sector:    0,
            file_offset:   0,
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

// ── Helpers ────────────────────────────────────────────────────────────────

/// Sector offset of a cluster's first sector in the data area.
fn cluster_to_lba(bpb: &Bpb, cluster: u16) -> u32 {
    bpb.data_start_lba + (cluster as u32 - 2) * bpb.sectors_per_cluster as u32
}

/// Read one 512-byte sector into a stack buffer. Returns false on error.
unsafe fn read_sector_buf(lba: u32, buf: &mut [u8; 512]) -> bool {
    unsafe { ata::read_sector(lba, buf) }
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

/// Initialise the FAT16 driver. Reads the BPB from sector 0.
///
/// Must be called after `ata::init()`.
pub unsafe fn init() {
    if !ata::is_present() {
        unsafe { SERIAL_PORT.write_str("FAT16: no ATA disk, skipping\n"); }
        return;
    }

    let mut buf = [0u8; 512];
    if !unsafe { read_sector_buf(0, &mut buf) } {
        unsafe { SERIAL_PORT.write_str("FAT16: failed to read boot sector\n"); }
        return;
    }

    // Validate FAT signature
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

    (*fs).bpb.fat_start_lba  = reserved;
    (*fs).bpb.root_dir_lba   = reserved + fat_count * fat_size;
    (*fs).bpb.data_start_lba = (*fs).bpb.root_dir_lba + (rde * 32 + 511) / 512;

    (*fs).ready = true;

    unsafe {
        SERIAL_PORT.write_str("FAT16: mounted, root_dir_lba=");
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

/// Open a file by path. Returns an FD ≥ 64 on success, negative on error.
pub unsafe fn open(raw_path: &[u8], _flags: u32) -> i64 {
    let fs = &raw mut FAT_FS;
    if !(*fs).ready { return -2; } // ENOSYS

    // Strip optional "/disk/" prefix
    let path = if raw_path.starts_with(b"/disk/") {
        &raw_path[6..]
    } else if raw_path.starts_with(b"/") {
        &raw_path[1..]
    } else {
        raw_path
    };

    if path.is_empty() { return -1; }

    // Split into name (up to 8 chars) and extension (up to 3 chars)
    let (name_part, ext_part) = match path.iter().position(|&b| b == b'.') {
        Some(dot) => (&path[..dot], &path[dot+1..]),
        None      => (path, &b""[..]),
    };

    let mut dir_name = [b' '; 11];
    for (i, &b) in name_part.iter().take(8).enumerate() {
        dir_name[i] = b.to_ascii_uppercase();
    }
    for (i, &b) in ext_part.iter().take(3).enumerate() {
        dir_name[8 + i] = b.to_ascii_uppercase();
    }

    // Search the root directory (each entry is 32 bytes, 16 per sector)
    let root_sectors = ((*fs).bpb.root_entry_count as u32 * 32 + 511) / 512;
    let mut buf = [0u8; 512];

    'outer: for s in 0..root_sectors {
        let lba = (*fs).bpb.root_dir_lba + s;
        if !unsafe { read_sector_buf(lba, &mut buf) } { return -1; }

        for e in 0..16u32 {
            let off = (e * 32) as usize;
            if buf[off] == 0x00 { break 'outer; }   // end of directory
            if buf[off] == 0xE5 { continue; }        // deleted entry
            let attr = buf[off + 11];
            if attr & 0x08 != 0 || attr & 0x10 != 0 { continue; } // skip volume/dir

            if buf[off..off+11] == dir_name {
                let first_cluster = u16::from_le_bytes([buf[off+26], buf[off+27]]);
                let file_size     = u32::from_le_bytes([buf[off+28], buf[off+29],
                                                         buf[off+30], buf[off+31]]);
                // Find a free FD slot
                let fds = &raw mut (*fs).fds;
                for i in 0..FAT_FD_COUNT {
                    let fd_slot = &raw mut (*fds)[i];
                    if !(*fd_slot).active {
                        (*fd_slot).active        = true;
                        (*fd_slot).file_size     = file_size;
                        (*fd_slot).first_cluster = first_cluster;
                        (*fd_slot).cur_cluster   = first_cluster;
                        (*fd_slot).cur_sector    = 0;
                        (*fd_slot).file_offset   = 0;
                        return (FAT_FD_BASE + i as i32) as i64;
                    }
                }
                return -4; // ENOMEM: no free FD slots
            }
        }
    }
    -7 // ENOENT
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

/// Close an open FAT FD.
pub unsafe fn close(fd: i32) -> i64 {
    let fs = &raw mut FAT_FS;
    if !is_fat_fd(fd) { return -5; }
    let idx = (fd - FAT_FD_BASE) as usize;
    let fds = &raw mut (*fs).fds;
    let slot = &raw mut (*fds)[idx];
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

/// List all entries in the FAT16 root directory.
///
/// Returns a `Vec` of `(name, is_directory)` pairs.  Volume-label and deleted
/// entries are skipped.  Returns an empty Vec if the filesystem is not ready.
pub unsafe fn list_root() -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    let fs = &raw const FAT_FS;
    if !(*fs).ready { return entries; }

    let bpb = &(*fs).bpb;
    let root_sectors = (bpb.root_entry_count as u32 * 32 + 511) / 512;
    let mut buf = [0u8; 512];

    'outer: for s in 0..root_sectors {
        let lba = bpb.root_dir_lba + s;
        if !unsafe { read_sector_buf(lba, &mut buf) } { break; }

        for e in 0..16u32 {
            let off = (e * 32) as usize;
            if buf[off] == 0x00 { break 'outer; }  // end of directory
            if buf[off] == 0xE5 { continue; }       // deleted entry
            let attr = buf[off + 11];
            if attr & 0x08 != 0 { continue; }       // volume label

            let is_dir = attr & 0x10 != 0;
            let name = fat83_to_string(&buf[off..off + 8], &buf[off + 8..off + 11]);
            if name == "." || name == ".." { continue; }
            entries.push((name, is_dir));
        }
    }
    entries
}
