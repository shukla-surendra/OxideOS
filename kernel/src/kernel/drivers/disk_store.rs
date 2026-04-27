//! Simple on-disk record store for OxideOS.
//!
//! Provides a persistent key-value store backed by any ATA disk.
//! Layout on disk (starting at `STORE_BASE_LBA = 2048`, 1 MB in):
//!
//!   LBA 2048       — store header (1 sector)
//!   LBA 2049..2303 — record slots (MAX_RECORDS = 255 slots × 1 sector each)
//!
//! Each record slot holds up to RECORD_DATA_MAX (496) bytes of user data.
//! Records are addressed by a u32 `id`.  On-disk format is:
//!
//!   [0..4)   magic:    u32  = RECORD_MAGIC if slot is used, 0 if empty
//!   [4..8)   id:       u32  record key
//!   [8..12)  data_len: u32  bytes used in the data region
//!   [12..16) checksum: u32  simple XOR of all data bytes
//!   [16..512) data:    [u8; 496]
//!
//! Up to four independent stores can exist, one per ATA disk position.

use super::ata;
use crate::kernel::serial::SERIAL_PORT;

// ── Constants ─────────────────────────────────────────────────────────────

const STORE_MAGIC:   u32 = 0x4F584453; // "OXDS"
const RECORD_MAGIC:  u32 = 0x52454358; // "RECX"
const STORE_BASE_LBA: u32 = 2048;      // 1 MB offset — safely past MBR / bootloader
const MAX_RECORDS:   u32 = 255;
pub const RECORD_DATA_MAX: usize = 496;

// ── On-disk structures (packed, fit in 512 bytes) ─────────────────────────

#[repr(C, packed)]
struct StoreHeader {
    magic:       u32,
    version:     u32,
    max_records: u32,
    data_size:   u32,  // RECORD_DATA_MAX
    _reserved:   [u8; 496],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct RecordSlot {
    pub magic:    u32,
    pub id:       u32,
    pub data_len: u32,
    pub checksum: u32,
    pub data:     [u8; RECORD_DATA_MAX],
}

const _: () = assert!(core::mem::size_of::<StoreHeader>() == 512);
const _: () = assert!(core::mem::size_of::<RecordSlot>()  == 512);

// ── Store state ───────────────────────────────────────────────────────────

static mut STORE_MOUNTED: [bool; 4] = [false; 4];

// ── Internal helpers ──────────────────────────────────────────────────────

fn header_lba(disk: usize) -> u32 { STORE_BASE_LBA }
fn record_lba(disk: usize, slot: u32) -> u32 { STORE_BASE_LBA + 1 + slot }

fn checksum(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |acc, &b| acc ^ b as u32)
}

unsafe fn read_slot(disk: usize, slot: u32) -> Option<RecordSlot> {
    let mut buf = [0u8; 512];
    if !unsafe { ata::read_sector(disk, record_lba(disk, slot), &mut buf) } {
        return None;
    }
    // SAFETY: RecordSlot is repr(C,packed) and 512 bytes — matches buf exactly.
    let s = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const RecordSlot) };
    Some(s)
}

unsafe fn write_slot(disk: usize, slot: u32, s: &RecordSlot) -> bool {
    let mut buf = [0u8; 512];
    unsafe { core::ptr::write_unaligned(buf.as_mut_ptr() as *mut RecordSlot, *s); }
    unsafe { ata::write_sector(disk, record_lba(disk, slot), &buf) }
}

// ── Public API ────────────────────────────────────────────────────────────

/// Mount (or format) the record store on disk `idx`.
///
/// If the store header is missing or corrupt the disk is formatted: the
/// header is written and all record slots are zeroed.  Returns `false` if
/// the disk is not present.
pub unsafe fn mount(disk: usize) -> bool {
    if !ata::is_present_at(disk) {
        unsafe { SERIAL_PORT.write_str("[store] disk not present\n"); }
        return false;
    }

    let mut buf = [0u8; 512];
    if !unsafe { ata::read_sector(disk, header_lba(disk), &mut buf) } {
        unsafe { SERIAL_PORT.write_str("[store] header read failed\n"); }
        return false;
    }

    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let ver   = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let maxr  = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);

    if magic == STORE_MAGIC && ver == 1 && maxr == MAX_RECORDS {
        unsafe {
            SERIAL_PORT.write_str("[store] mounted on disk");
            SERIAL_PORT.write_decimal(disk as u32);
            SERIAL_PORT.write_str("\n");
            STORE_MOUNTED[disk] = true;
        }
        return true;
    }

    // Format: write header.
    unsafe { SERIAL_PORT.write_str("[store] formatting disk"); }
    unsafe { SERIAL_PORT.write_decimal(disk as u32); }
    unsafe { SERIAL_PORT.write_str("...\n"); }

    let hdr = StoreHeader {
        magic:       STORE_MAGIC,
        version:     1,
        max_records: MAX_RECORDS,
        data_size:   RECORD_DATA_MAX as u32,
        _reserved:   [0; 496],
    };
    let mut hdr_buf = [0u8; 512];
    unsafe { core::ptr::write_unaligned(hdr_buf.as_mut_ptr() as *mut StoreHeader, hdr); }
    if !unsafe { ata::write_sector(disk, header_lba(disk), &hdr_buf) } {
        return false;
    }

    // Zero all record slots.
    let empty = [0u8; 512];
    for slot in 0..MAX_RECORDS {
        let _ = unsafe { ata::write_sector(disk, record_lba(disk, slot), &empty) };
    }

    unsafe { STORE_MOUNTED[disk] = true; }
    true
}

/// Write a record to the store.  `id` is the key, `data` up to 496 bytes.
/// Overwrites an existing record with the same id.
/// Returns `false` if the disk is not mounted or the store is full.
pub unsafe fn write_record(disk: usize, id: u32, data: &[u8]) -> bool {
    if !unsafe { STORE_MOUNTED[disk] } { return false; }
    let data_len = data.len().min(RECORD_DATA_MAX);

    // Find existing slot with this id, or the first empty slot.
    let mut target_slot: Option<u32> = None;
    let mut first_empty: Option<u32> = None;

    for slot in 0..MAX_RECORDS {
        match unsafe { read_slot(disk, slot) } {
            Some(s) if s.magic == RECORD_MAGIC && s.id == id => {
                target_slot = Some(slot);
                break;
            }
            Some(s) if s.magic != RECORD_MAGIC && first_empty.is_none() => {
                first_empty = Some(slot);
            }
            _ => {}
        }
    }

    let slot = target_slot.or(first_empty);
    let slot = match slot {
        Some(s) => s,
        None => {
            unsafe { SERIAL_PORT.write_str("[store] no free slot\n"); }
            return false;
        }
    };

    let mut rec = RecordSlot {
        magic:    RECORD_MAGIC,
        id,
        data_len: data_len as u32,
        checksum: 0,
        data:     [0u8; RECORD_DATA_MAX],
    };
    rec.data[..data_len].copy_from_slice(&data[..data_len]);
    rec.checksum = checksum(&rec.data[..data_len]);

    unsafe { write_slot(disk, slot, &rec) }
}

/// Read a record from the store by `id`.
/// Returns the number of data bytes copied into `buf`, or `None` if not found.
pub unsafe fn read_record(disk: usize, id: u32, buf: &mut [u8]) -> Option<usize> {
    if !unsafe { STORE_MOUNTED[disk] } { return None; }

    for slot in 0..MAX_RECORDS {
        let s = unsafe { read_slot(disk, slot)? };
        if s.magic != RECORD_MAGIC || s.id != id { continue; }

        let dlen = (s.data_len as usize).min(RECORD_DATA_MAX);
        // Verify checksum.
        if checksum(&s.data[..dlen]) != s.checksum {
            unsafe { SERIAL_PORT.write_str("[store] checksum mismatch\n"); }
            return None;
        }

        let copy = dlen.min(buf.len());
        buf[..copy].copy_from_slice(&s.data[..copy]);
        return Some(copy);
    }
    None
}

/// Delete a record by `id`.  Returns `true` if the record existed.
pub unsafe fn delete_record(disk: usize, id: u32) -> bool {
    if !unsafe { STORE_MOUNTED[disk] } { return false; }

    for slot in 0..MAX_RECORDS {
        if let Some(s) = unsafe { read_slot(disk, slot) } {
            if s.magic == RECORD_MAGIC && s.id == id {
                let empty = [0u8; 512];
                return unsafe { ata::write_sector(disk, record_lba(disk, slot), &empty) };
            }
        }
    }
    false
}

/// Fill `out` with the ids of all records present in the store.
/// Returns the number of records found.
pub unsafe fn list_records(disk: usize, out: &mut [u32]) -> usize {
    if !unsafe { STORE_MOUNTED[disk] } { return 0; }
    let mut count = 0usize;

    for slot in 0..MAX_RECORDS {
        if count >= out.len() { break; }
        if let Some(s) = unsafe { read_slot(disk, slot) } {
            if s.magic == RECORD_MAGIC {
                out[count] = s.id;
                count += 1;
            }
        }
    }
    count
}

/// Returns `true` if the store on `disk` is mounted.
pub fn is_mounted(disk: usize) -> bool {
    unsafe { STORE_MOUNTED[disk] }
}
