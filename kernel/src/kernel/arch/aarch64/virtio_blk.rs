//! virtio-blk driver over virtio-mmio — polled, no GIC required.
//!
//! Storage counterpart of `virtio_input.rs`: we probe the QEMU `virt`
//! machine's 32 virtio-mmio slots for block devices (device ID 2), bring
//! each one to DRIVER_OK with a single request queue, and issue one
//! synchronous request at a time — build the three-descriptor chain
//! (header → 512-byte data → status), publish it, kick the queue, then spin
//! on the used ring until the device retires it.  Polling matches the rest
//! of the aarch64 port until the GIC lands; QEMU completes requests in
//! microseconds, so the spin is short.
//!
//! The public API mirrors `drivers/ata.rs` exactly, and this module is
//! re-exported as the flat `crate::kernel::ata` path on aarch64 — so the
//! portable storage stack (mbr, fat, disk_store, diskfs) and its GUI
//! callers compile and run unchanged on top of it.  Following the x86
//! convention, the first block device found is logical disk 0 (FAT16 /
//! record store) and the second is logical disk 3 (the "secondary" slot
//! the ext2/installer path uses).
//!
//! Registers are reached through the Limine HHDM; ring and request buffers
//! come from the kernel heap (also in the HHDM), so `phys = virt - hhdm`.

use alloc::alloc::{alloc_zeroed, Layout};
use core::sync::atomic::{fence, Ordering};

use crate::kernel::serial::SERIAL_PORT;

// ── QEMU virt virtio-mmio window (same as virtio-input) ──────────────────────

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: u64 = 32;

// ── virtio-mmio registers (offsets from a slot base) ─────────────────────────

const REG_MAGIC: u64 = 0x000;
const REG_VERSION: u64 = 0x004;
const REG_DEVICE_ID: u64 = 0x008;
const REG_DRV_FEATURES: u64 = 0x020;
const REG_DRV_FEATURES_SEL: u64 = 0x024;
const REG_GUEST_PAGE_SIZE: u64 = 0x028; // legacy only
const REG_QUEUE_SEL: u64 = 0x030;
const REG_QUEUE_NUM_MAX: u64 = 0x034;
const REG_QUEUE_NUM: u64 = 0x038;
const REG_QUEUE_ALIGN: u64 = 0x03c; // legacy only
const REG_QUEUE_PFN: u64 = 0x040; // legacy only
const REG_QUEUE_READY: u64 = 0x044; // modern only
const REG_QUEUE_NOTIFY: u64 = 0x050;
const REG_INT_STATUS: u64 = 0x060;
const REG_INT_ACK: u64 = 0x064;
const REG_STATUS: u64 = 0x070;
const REG_QUEUE_DESC_LOW: u64 = 0x080; // modern only
const REG_QUEUE_DESC_HIGH: u64 = 0x084;
const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
const REG_CONFIG: u64 = 0x100;

const MAGIC_VIRT: u32 = 0x7472_6976;
const DEVICE_ID_BLOCK: u32 = 2;

const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;

// Virtqueue constants.
const QUEUE_REQ: u32 = 0;
const QUEUE_SIZE: u16 = 8; // we use one 3-descriptor chain at a time
const PAGE_SIZE: u64 = 4096;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

// virtio-blk request header types / status codes.
const BLK_T_IN: u32 = 0; // device → driver (disk read)
const BLK_T_OUT: u32 = 1; // driver → device (disk write)
const BLK_S_OK: u8 = 0;

/// How long to spin on the used ring before declaring a request lost.
const POLL_SPIN_LIMIT: u32 = 100_000_000;

/// Split virtqueue descriptor (virtio spec 2.7.5).
#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// One initialised virtio-blk device with a single reusable request chain.
struct BlkDev {
    regs: u64, // virtual base of the MMIO register block
    desc: *mut VirtqDesc,
    avail: *mut u8, // flags u16, idx u16, ring[QUEUE_SIZE] u16
    used: *mut u8,  // flags u16, idx u16, ring[QUEUE_SIZE] {id u32, len u32}
    req: *mut u8,   // request page: header @0, status @16, data @512
    avail_idx: u16,
    last_used: u16,
    capacity: u64, // device size in 512-byte sectors
}

/// Logical disk table with ata.rs index semantics: slot 0 = primary data
/// disk (FAT16 + record store), slot 3 = secondary disk.  Probe order maps
/// the first device found to 0, the second to 3, then 1 and 2.
static mut DISKS: [Option<BlkDev>; 4] = [None, None, None, None];
const PROBE_TO_LOGICAL: [usize; 4] = [0, 3, 1, 2];

// Request-page offsets.
const REQ_HDR: usize = 0; // 16-byte {type u32, reserved u32, sector u64}
const REQ_STATUS: usize = 16; // 1-byte status, device-written
const REQ_DATA: usize = 512; // 512-byte sector buffer

// ── MMIO helpers ─────────────────────────────────────────────────────────────

unsafe fn reg_read(base: u64, off: u64) -> u32 {
    unsafe { ((base + off) as *const u32).read_volatile() }
}

unsafe fn reg_write(base: u64, off: u64, val: u32) {
    unsafe { ((base + off) as *mut u32).write_volatile(val) }
}

unsafe fn read_u16(p: *const u8) -> u16 {
    unsafe { (p as *const u16).read_volatile() }
}

unsafe fn write_u16(p: *mut u8, v: u16) {
    unsafe { (p as *mut u16).write_volatile(v) }
}

fn disks() -> &'static mut [Option<BlkDev>; 4] {
    unsafe { &mut *core::ptr::addr_of_mut!(DISKS) }
}

// ── Initialisation ───────────────────────────────────────────────────────────

/// Probe the virtio-mmio window and bring up every block device found.
/// Requires the heap and the Limine HHDM.  Call once during boot.
pub unsafe fn init() {
    let Some(hhdm) = crate::HHDM_REQUEST.get_response().map(|r| r.offset()) else {
        unsafe { SERIAL_PORT.write_str("virtio-blk: no HHDM response, skipping\n"); }
        return;
    };

    let mut found = 0usize;
    for slot in 0..MMIO_SLOTS {
        let base = hhdm + MMIO_BASE + slot * MMIO_STRIDE;
        unsafe {
            if reg_read(base, REG_MAGIC) != MAGIC_VIRT {
                continue;
            }
            if reg_read(base, REG_DEVICE_ID) != DEVICE_ID_BLOCK {
                continue;
            }
            let version = reg_read(base, REG_VERSION);
            if version != 1 && version != 2 {
                continue;
            }
            if found >= PROBE_TO_LOGICAL.len() {
                break;
            }
            if let Some(dev) = init_device(base, version, hhdm) {
                let logical = PROBE_TO_LOGICAL[found];
                SERIAL_PORT.write_str("virtio-blk: disk");
                SERIAL_PORT.write_decimal(logical as u32);
                SERIAL_PORT.write_str(" ready, ");
                SERIAL_PORT.write_decimal((dev.capacity / 2048) as u32);
                SERIAL_PORT.write_str(" MB\n");
                disks()[logical] = Some(dev);
                found += 1;
            }
        }
    }

    if found == 0 {
        unsafe { SERIAL_PORT.write_str("virtio-blk: no block devices found\n"); }
    }
}

/// Bring one device from reset to DRIVER_OK with a populated request queue.
unsafe fn init_device(base: u64, version: u32, hhdm: u64) -> Option<BlkDev> {
    unsafe {
        let legacy = version == 1;

        // Status dance: reset → ACKNOWLEDGE → DRIVER.
        reg_write(base, REG_STATUS, 0);
        reg_write(base, REG_STATUS, STATUS_ACKNOWLEDGE);
        reg_write(base, REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // Feature negotiation: we need none of the optional block features
        // (flush, geometry, …); modern devices require VIRTIO_F_VERSION_1.
        reg_write(base, REG_DRV_FEATURES_SEL, 1);
        reg_write(base, REG_DRV_FEATURES, if legacy { 0 } else { 1 });
        reg_write(base, REG_DRV_FEATURES_SEL, 0);
        reg_write(base, REG_DRV_FEATURES, 0);

        let mut status = STATUS_ACKNOWLEDGE | STATUS_DRIVER;
        if !legacy {
            status |= STATUS_FEATURES_OK;
            reg_write(base, REG_STATUS, status);
            if reg_read(base, REG_STATUS) & STATUS_FEATURES_OK == 0 {
                SERIAL_PORT.write_str("virtio-blk: device rejected features\n");
                reg_write(base, REG_STATUS, STATUS_FAILED);
                return None;
            }
        }

        // Request queue geometry.
        reg_write(base, REG_QUEUE_SEL, QUEUE_REQ);
        let max = reg_read(base, REG_QUEUE_NUM_MAX);
        if max == 0 {
            reg_write(base, REG_STATUS, STATUS_FAILED);
            return None;
        }
        let qsize = QUEUE_SIZE.min(max as u16);

        // Ring memory in the legacy contiguous layout (descriptor table,
        // avail ring, page-aligned used ring) — works for modern too since
        // the three addresses are passed separately there.
        let desc_bytes = 16 * qsize as u64;
        let avail_bytes = 6 + 2 * qsize as u64;
        let used_off = (desc_bytes + avail_bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let used_bytes = 6 + 8 * qsize as u64;
        let ring_total = (used_off + used_bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        let ring = alloc_zeroed(Layout::from_size_align(ring_total as usize, PAGE_SIZE as usize).ok()?);
        if ring.is_null() {
            return None;
        }
        let ring_phys = ring as u64 - hhdm;

        // One page holds the request header, status byte and sector buffer.
        let req = alloc_zeroed(Layout::from_size_align(PAGE_SIZE as usize, PAGE_SIZE as usize).ok()?);
        if req.is_null() {
            return None;
        }
        let req_phys = req as u64 - hhdm;

        let desc = ring as *mut VirtqDesc;
        let avail = ring.add(desc_bytes as usize);
        let used = ring.add(used_off as usize);

        // Permanent 3-descriptor chain: header → data → status.  Only the
        // data descriptor's WRITE flag and lengths ever change per request.
        desc.add(0).write(VirtqDesc {
            addr: req_phys + REQ_HDR as u64,
            len: 16,
            flags: DESC_F_NEXT,
            next: 1,
        });
        desc.add(1).write(VirtqDesc {
            addr: req_phys + REQ_DATA as u64,
            len: 512,
            flags: DESC_F_NEXT | DESC_F_WRITE,
            next: 2,
        });
        desc.add(2).write(VirtqDesc {
            addr: req_phys + REQ_STATUS as u64,
            len: 1,
            flags: DESC_F_WRITE,
            next: 0,
        });

        reg_write(base, REG_QUEUE_NUM, qsize as u32);
        if legacy {
            reg_write(base, REG_GUEST_PAGE_SIZE, PAGE_SIZE as u32);
            reg_write(base, REG_QUEUE_ALIGN, PAGE_SIZE as u32);
            reg_write(base, REG_QUEUE_PFN, (ring_phys / PAGE_SIZE) as u32);
        } else {
            let avail_phys = ring_phys + desc_bytes;
            let used_phys = ring_phys + used_off;
            reg_write(base, REG_QUEUE_DESC_LOW, ring_phys as u32);
            reg_write(base, REG_QUEUE_DESC_HIGH, (ring_phys >> 32) as u32);
            reg_write(base, REG_QUEUE_DRIVER_LOW, avail_phys as u32);
            reg_write(base, REG_QUEUE_DRIVER_HIGH, (avail_phys >> 32) as u32);
            reg_write(base, REG_QUEUE_DEVICE_LOW, used_phys as u32);
            reg_write(base, REG_QUEUE_DEVICE_HIGH, (used_phys >> 32) as u32);
            reg_write(base, REG_QUEUE_READY, 1);
        }

        reg_write(base, REG_STATUS, status | STATUS_DRIVER_OK);

        // Capacity (in 512-byte sectors) lives at config offset 0 on both
        // legacy and modern devices; read as two u32 halves.
        let cap_lo = reg_read(base, REG_CONFIG) as u64;
        let cap_hi = reg_read(base, REG_CONFIG + 4) as u64;

        Some(BlkDev {
            regs: base,
            desc,
            avail,
            used,
            req,
            avail_idx: 0,
            last_used: 0,
            capacity: (cap_hi << 32) | cap_lo,
        })
    }
}

// ── Synchronous request path ─────────────────────────────────────────────────

/// Submit one read or write for `lba` and spin until the device retires it.
/// Returns true when the device reports VIRTIO_BLK_S_OK.
unsafe fn request(dev: &mut BlkDev, write: bool, lba: u64) -> bool {
    unsafe {
        // Header: {type, reserved, sector}.
        (dev.req.add(REQ_HDR) as *mut u32).write_volatile(if write { BLK_T_OUT } else { BLK_T_IN });
        (dev.req.add(REQ_HDR + 4) as *mut u32).write_volatile(0);
        (dev.req.add(REQ_HDR + 8) as *mut u64).write_volatile(lba);
        // Poison the status byte so we can tell the device really wrote it.
        dev.req.add(REQ_STATUS).write_volatile(0xFF);

        // Data descriptor direction: device writes it on reads, reads it on writes.
        let data_desc = dev.desc.add(1);
        let flags = if write { DESC_F_NEXT } else { DESC_F_NEXT | DESC_F_WRITE };
        core::ptr::addr_of_mut!((*data_desc).flags).write_volatile(flags);

        // Publish descriptor 0 (chain head) in the avail ring and kick.
        let qsize = QUEUE_SIZE; // ring was sized with this cap in init_device
        let slot = (dev.avail_idx % qsize) as usize;
        write_u16(dev.avail.add(4 + 2 * slot), 0);
        fence(Ordering::Release);
        dev.avail_idx = dev.avail_idx.wrapping_add(1);
        write_u16(dev.avail.add(2), dev.avail_idx);
        fence(Ordering::Release);
        reg_write(dev.regs, REG_QUEUE_NOTIFY, QUEUE_REQ);

        // Spin until the used ring advances (polled — no GIC yet).
        let mut spins = 0u32;
        loop {
            fence(Ordering::Acquire);
            if read_u16(dev.used.add(2)) != dev.last_used {
                break;
            }
            spins += 1;
            if spins >= POLL_SPIN_LIMIT {
                SERIAL_PORT.write_str("virtio-blk: request timed out\n");
                return false;
            }
            core::hint::spin_loop();
        }
        dev.last_used = dev.last_used.wrapping_add(1);

        // Ack the interrupt bit so state stays clean for a future GIC version.
        let isr = reg_read(dev.regs, REG_INT_STATUS);
        if isr != 0 {
            reg_write(dev.regs, REG_INT_ACK, isr);
        }

        dev.req.add(REQ_STATUS).read_volatile() == BLK_S_OK
    }
}

// ── Public API (mirrors drivers/ata.rs) ──────────────────────────────────────

pub fn is_present() -> bool {
    disks()[0].is_some()
}
pub fn is_present_sec() -> bool {
    disks()[3].is_some()
}
pub fn is_present_at(idx: usize) -> bool {
    idx < 4 && disks()[idx].is_some()
}
pub fn sector_count() -> u32 {
    disks()[0].as_ref().map(|d| d.capacity as u32).unwrap_or(0)
}
pub fn sector_count_sec() -> u32 {
    disks()[3].as_ref().map(|d| d.capacity as u32).unwrap_or(0)
}
pub fn disk_count() -> usize {
    disks().iter().filter(|d| d.is_some()).count()
}
/// (sectors, slave, lba48) — the last two only exist for ATA; report
/// not-slave and full-LBA so diskfs prints sensible values.
pub fn disk_info(idx: usize) -> Option<(u64, bool, bool)> {
    if idx >= 4 {
        return None;
    }
    disks()[idx].as_ref().map(|d| (d.capacity, false, true))
}

/// Read one 512-byte sector from logical disk `idx` at the given LBA.
pub unsafe fn read_sector(idx: usize, lba: u32, buf: &mut [u8; 512]) -> bool {
    if idx >= 4 {
        return false;
    }
    let Some(dev) = disks()[idx].as_mut() else {
        return false;
    };
    if (lba as u64) >= dev.capacity {
        return false;
    }
    if !unsafe { request(dev, false, lba as u64) } {
        return false;
    }
    unsafe {
        fence(Ordering::Acquire);
        core::ptr::copy_nonoverlapping(dev.req.add(REQ_DATA), buf.as_mut_ptr(), 512);
    }
    true
}

/// Write one 512-byte sector to logical disk `idx` at the given LBA.
pub unsafe fn write_sector(idx: usize, lba: u32, buf: &[u8; 512]) -> bool {
    if idx >= 4 {
        return false;
    }
    let Some(dev) = disks()[idx].as_mut() else {
        return false;
    };
    if (lba as u64) >= dev.capacity {
        return false;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), dev.req.add(REQ_DATA), 512);
        fence(Ordering::Release);
        request(dev, true, lba as u64)
    }
}

// ── Legacy per-bus wrappers (ata.rs parity) ──────────────────────────────────

pub unsafe fn read_sector_sec(lba: u32, buf: &mut [u8; 512]) -> bool {
    unsafe { read_sector(3, lba, buf) }
}
pub unsafe fn write_sector_sec(lba: u32, buf: &[u8; 512]) -> bool {
    unsafe { write_sector(3, lba, buf) }
}

/// Read `count` consecutive sectors into `buf` (must hold count*512 bytes).
pub unsafe fn read_sectors(idx: usize, lba: u32, count: u32, buf: &mut [u8]) -> bool {
    for i in 0..count {
        let chunk = &mut buf[(i as usize) * 512..][..512];
        let chunk: &mut [u8; 512] = chunk.try_into().unwrap();
        if !unsafe { read_sector(idx, lba + i, chunk) } {
            return false;
        }
    }
    true
}

/// Write `count` consecutive sectors from `buf` (must hold count*512 bytes).
pub unsafe fn write_sectors(idx: usize, lba: u32, count: u32, buf: &[u8]) -> bool {
    for i in 0..count {
        let chunk = &buf[(i as usize) * 512..][..512];
        let chunk: &[u8; 512] = chunk.try_into().unwrap();
        if !unsafe { write_sector(idx, lba + i, chunk) } {
            return false;
        }
    }
    true
}
