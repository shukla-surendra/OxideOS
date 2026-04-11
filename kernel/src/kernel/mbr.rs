//! MBR (Master Boot Record) partition table parser for OxideOS.
//!
//! Reads LBA 0 of the primary ATA disk to detect whether it is:
//!   (a) Whole-disk formatted (FAT16 BPB at LBA 0, byte 0 = 0xEB/0xE9), or
//!   (b) MBR-partitioned (partition table at bytes 446–509, 0x55AA at 510–511).
//!
//! The parsed state is stored as a static and is queried by filesystem drivers
//! during their own `init()` calls to find their partition LBA offset.

use crate::kernel::ata;
use crate::kernel::serial::SERIAL_PORT;

// ── Partition type bytes we care about ─────────────────────────────────────
/// FAT16 with CHS (<32 MB)
pub const PTYPE_FAT16_SMALL: u8 = 0x04;
/// FAT16 with CHS (≥32 MB)
pub const PTYPE_FAT16_LARGE: u8 = 0x06;
/// FAT16 with LBA addressing
pub const PTYPE_FAT16_LBA:   u8 = 0x0E;
/// Linux ext2/ext3/ext4
pub const PTYPE_LINUX:       u8 = 0x83;

// ── Data structures ────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct PartEntry {
    pub status:         u8,  // 0x80 = bootable, 0x00 = inactive
    pub partition_type: u8,
    pub start_lba:      u32,
    pub size_sectors:   u32,
}

impl PartEntry {
    const fn empty() -> Self {
        Self { status: 0, partition_type: 0, start_lba: 0, size_sectors: 0 }
    }
    pub fn is_empty(&self) -> bool { self.partition_type == 0 && self.start_lba == 0 }
}

pub struct MbrState {
    pub entries:    [PartEntry; 4],
    /// `true` when LBA 0 holds a whole-disk filesystem BPB with no partition table.
    pub whole_disk: bool,
    /// `true` once `init()` has successfully read LBA 0.
    pub valid:      bool,
}

impl MbrState {
    const fn new() -> Self {
        Self {
            entries: [
                PartEntry::empty(),
                PartEntry::empty(),
                PartEntry::empty(),
                PartEntry::empty(),
            ],
            whole_disk: true,
            valid: false,
        }
    }
}

pub static mut MBR: MbrState = MbrState::new();

// ── Initialisation ──────────────────────────────────────────────────────────

/// Parse the partition table at LBA 0.  Must be called after `ata::init()`.
pub unsafe fn init() {
    if !ata::is_present() { return; }

    let mut buf = [0u8; 512];
    if !unsafe { ata::read_sector(0, &mut buf) } {
        unsafe { SERIAL_PORT.write_str("MBR: failed to read LBA 0\n"); }
        return;
    }

    // Both a FAT16 BPB and a real MBR have 0x55/0xAA at bytes 510/511.
    if buf[510] != 0x55 || buf[511] != 0xAA {
        unsafe { SERIAL_PORT.write_str("MBR: no 0x55AA — unformatted disk?\n"); }
        return;
    }

    let mbr = &raw mut MBR;
    (*mbr).valid = true;

    // A FAT16 BPB starts with a JMP SHORT (0xEB) or JMP NEAR (0xE9) instruction.
    // Real MBR bootstrap code does not.
    if buf[0] == 0xEB || buf[0] == 0xE9 {
        (*mbr).whole_disk = true;
        unsafe { SERIAL_PORT.write_str("MBR: whole-disk format (FAT BPB at LBA 0)\n"); }
        return;
    }

    // Parse four 16-byte entries at offset 446.
    let mut has_any = false;
    for i in 0..4usize {
        let off = 446 + i * 16;
        let pt    = buf[off + 4];
        let start = u32::from_le_bytes([buf[off+8],  buf[off+9],  buf[off+10], buf[off+11]]);
        let size  = u32::from_le_bytes([buf[off+12], buf[off+13], buf[off+14], buf[off+15]]);
        (*mbr).entries[i] = PartEntry {
            status: buf[off], partition_type: pt, start_lba: start, size_sectors: size,
        };
        if pt != 0 { has_any = true; }
    }

    if has_any {
        (*mbr).whole_disk = false;
        unsafe {
            SERIAL_PORT.write_str("MBR: partition table:\n");
            for i in 0..4 {
                let e = &(*mbr).entries[i];
                if e.partition_type != 0 {
                    SERIAL_PORT.write_str("  [");
                    SERIAL_PORT.write_decimal(i as u32);
                    SERIAL_PORT.write_str("] type=0x");
                    SERIAL_PORT.write_hex(e.partition_type as u32);
                    SERIAL_PORT.write_str(" lba=");
                    SERIAL_PORT.write_decimal(e.start_lba);
                    SERIAL_PORT.write_str(" size=");
                    SERIAL_PORT.write_decimal(e.size_sectors);
                    SERIAL_PORT.write_str(" sectors\n");
                }
            }
        }
    } else {
        // All entries empty — fall back to whole-disk mode.
        (*mbr).whole_disk = true;
        unsafe { SERIAL_PORT.write_str("MBR: no partition entries, whole-disk mode\n"); }
    }
}

// ── Query helpers ───────────────────────────────────────────────────────────

/// LBA offset for the FAT16 partition.  Returns `0` for whole-disk FAT.
pub unsafe fn fat16_lba_offset() -> u32 {
    let mbr = &raw const MBR;
    if !(*mbr).valid || (*mbr).whole_disk { return 0; }
    for e in &(*mbr).entries {
        if matches!(e.partition_type, PTYPE_FAT16_SMALL | PTYPE_FAT16_LARGE | PTYPE_FAT16_LBA) {
            return e.start_lba;
        }
    }
    0
}

/// LBA start of the Linux/ext2 partition, or `None` if not present.
pub unsafe fn ext2_lba_offset() -> Option<u32> {
    let mbr = &raw const MBR;
    if !(*mbr).valid || (*mbr).whole_disk { return None; }
    for e in &(*mbr).entries {
        if e.partition_type == PTYPE_LINUX {
            return Some(e.start_lba);
        }
    }
    None
}
