//! ATA PIO driver — primary and secondary buses, master and slave drives.
//!
//! Probes all four positions:
//!   DISKS[0] = Primary   Master  (0x1F0 / 0x3F6)
//!   DISKS[1] = Primary   Slave   (0x1F0 / 0x3F6, device-select bit 4)
//!   DISKS[2] = Secondary Master  (0x170 / 0x376)
//!   DISKS[3] = Secondary Slave   (0x170 / 0x376, device-select bit 4)
//!
//! Each position is probed independently. Backward-compatible wrapper
//! functions keep the old callers (installer, terminal, main) unchanged.
//!
//! Works with: QEMU -device ide-hd, VirtualBox IDE controller,
//!             VMware IDE adapter (set controller to IDE in VM settings).

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

// ── Status / command bits ─────────────────────────────────────────────────

const SR_ERR: u8 = 1 << 0;
const SR_DRQ: u8 = 1 << 3;
const SR_BSY: u8 = 1 << 7;

// ── ATA commands ──────────────────────────────────────────────────────────

const CMD_IDENTIFY: u8 = 0xEC;
const CMD_READ:     u8 = 0x20; // READ SECTORS (LBA28)
const CMD_WRITE:    u8 = 0x30; // WRITE SECTORS (LBA28)
const CMD_FLUSH:    u8 = 0xE7;
const CMD_READ48:   u8 = 0x24; // READ SECTORS EXT (LBA48)
const CMD_WRITE48:  u8 = 0x34; // WRITE SECTORS EXT (LBA48)
const CMD_FLUSH48:  u8 = 0xEA;

// ── Bus configurations ────────────────────────────────────────────────────

const PRIMARY_IO:   u16 = 0x1F0;
const PRIMARY_CTRL: u16 = 0x3F6;
const SECONDARY_IO:   u16 = 0x170;
const SECONDARY_CTRL: u16 = 0x376;

// ── Per-disk offsets from io_base ─────────────────────────────────────────

const OFF_DATA:    u16 = 0; // 0x1F0 / 0x170
const OFF_FEAT:    u16 = 1; // write: features
const OFF_SECNT0:  u16 = 2;
const OFF_LBA0:    u16 = 3;
const OFF_LBA1:    u16 = 4;
const OFF_LBA2:    u16 = 5;
const OFF_DRVHD:   u16 = 6;
const OFF_CMD:     u16 = 7; // read: status, write: command

// ── Disk descriptor ───────────────────────────────────────────────────────

pub struct AtaDisk {
    pub io_base:  u16,
    pub ctrl:     u16,
    pub slave:    bool,       // false = master (device 0), true = slave (device 1)
    pub sectors:  u64,        // total LBA sector count (LBA48 allows >2G)
    pub lba48:    bool,
    pub model:    [u8; 40],
}

// ── Global disk table ─────────────────────────────────────────────────────

pub static mut DISKS: [Option<AtaDisk>; 4] = [None, None, None, None];

// ── Port helpers ──────────────────────────────────────────────────────────

#[inline] unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") v, options(nomem, nostack)); }
    v
}
#[inline] unsafe fn inw(port: u16) -> u16 {
    let v: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") v, options(nomem, nostack)); }
    v
}
#[inline] unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack)); }
}
#[inline] unsafe fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack)); }
}

// ── Per-disk helpers (use io_base + offset) ───────────────────────────────

unsafe fn disk_inb(io: u16, off: u16) -> u8    { unsafe { inb(io + off) } }
unsafe fn disk_inw(io: u16, off: u16) -> u16   { unsafe { inw(io + off) } }
unsafe fn disk_outb(io: u16, off: u16, v: u8)  { unsafe { outb(io + off, v) } }
unsafe fn disk_outw(io: u16, off: u16, v: u16) { unsafe { outw(io + off, v) } }

/// 400 ns delay — 4 reads of the alt-status register.
unsafe fn delay400ns(ctrl: u16) {
    for _ in 0..4 { let _ = unsafe { inb(ctrl) }; }
}

unsafe fn wait_busy(io: u16, timeout: u32) -> bool {
    for _ in 0..timeout {
        if unsafe { disk_inb(io, OFF_CMD) } & SR_BSY == 0 { return true; }
    }
    false
}

unsafe fn wait_drq(io: u16, timeout: u32) -> bool {
    for _ in 0..timeout {
        let s = unsafe { disk_inb(io, OFF_CMD) };
        if s & SR_ERR != 0 { return false; }
        if s & SR_DRQ != 0 { return true;  }
    }
    false
}

// ── Probing ───────────────────────────────────────────────────────────────

unsafe fn probe_disk(slot: usize, io: u16, ctrl: u16, slave: bool) {
    let dev_sel: u8 = if slave { 0xB0 } else { 0xA0 };

    // Soft reset the bus (affects both master and slave).
    if !slave {
        unsafe {
            outb(ctrl, 0x02); // nIEN
            outb(ctrl, 0x06); // SRST | nIEN
            for _ in 0..20 { let _ = inb(ctrl); }
            outb(ctrl, 0x02); // clear SRST
        }
        // Wait for BSY to clear after reset.
        if !unsafe { wait_busy(io, 10_000_000) } {
            unsafe { SERIAL_PORT.write_str("[ata] bus reset timeout\n"); }
            return;
        }
    }

    // Select drive.
    unsafe { disk_outb(io, OFF_DRVHD, dev_sel); }
    unsafe { delay400ns(ctrl); }

    // Floating bus check.
    let status = unsafe { disk_inb(io, OFF_CMD) };
    if status == 0xFF {
        return; // no controller on this bus
    }

    // Clear IDENTIFY registers.
    unsafe {
        disk_outb(io, OFF_FEAT,  0);
        disk_outb(io, OFF_SECNT0, 0);
        disk_outb(io, OFF_LBA0,  0);
        disk_outb(io, OFF_LBA1,  0);
        disk_outb(io, OFF_LBA2,  0);
    }

    // Issue IDENTIFY.
    unsafe { disk_outb(io, OFF_CMD, CMD_IDENTIFY); }
    unsafe { delay400ns(ctrl); }

    let status = unsafe { disk_inb(io, OFF_CMD) };
    if status == 0x00 {
        return; // no drive
    }

    if !unsafe { wait_busy(io, 1_000_000) } {
        return; // stuck BSY — no drive or broken
    }

    // ATAPI devices have non-zero LBA1/LBA2 after IDENTIFY.
    let sig1 = unsafe { disk_inb(io, OFF_LBA1) };
    let sig2 = unsafe { disk_inb(io, OFF_LBA2) };
    if (sig1 != 0 || sig2 != 0) && !(sig1 == 0x3C && sig2 == 0xC3) {
        // ATAPI or SATA signature — skip for now.
        unsafe {
            SERIAL_PORT.write_str("[ata] slot ");
            SERIAL_PORT.write_decimal(slot as u32);
            SERIAL_PORT.write_str(": ATAPI/SATA sig=");
            SERIAL_PORT.write_hex(((sig2 as u32) << 8) | sig1 as u32);
            SERIAL_PORT.write_str("\n");
        }
        return;
    }

    if !unsafe { wait_drq(io, 1_000_000) } {
        return; // no data
    }

    // Read 256-word IDENTIFY response.
    let mut id = [0u16; 256];
    for w in id.iter_mut() { *w = unsafe { disk_inw(io, OFF_DATA) }; }

    // LBA28 sector count: words 60–61.
    let lba28 = (id[60] as u64) | ((id[61] as u64) << 16);

    // LBA48: word 83 bit 10 = LBA48 supported; sector count words 100–103.
    let lba48 = (id[83] & (1 << 10)) != 0;
    let sectors = if lba48 {
        (id[100] as u64)
        | ((id[101] as u64) << 16)
        | ((id[102] as u64) << 32)
        | ((id[103] as u64) << 48)
    } else {
        lba28
    };

    if sectors == 0 { return; }

    // Extract model string (words 27–46, big-endian byte pairs).
    let mut model = [b' '; 40];
    for i in 0..20usize {
        let w = id[27 + i];
        model[i * 2]     = (w >> 8) as u8;
        model[i * 2 + 1] = (w & 0xFF) as u8;
    }
    // Trim trailing spaces.
    let mut end = 40;
    while end > 0 && model[end - 1] == b' ' { end -= 1; }

    unsafe {
        SERIAL_PORT.write_str("[ata] disk");
        SERIAL_PORT.write_decimal(slot as u32);
        SERIAL_PORT.write_str(" (");
        SERIAL_PORT.write_str(if slave { "slave" } else { "master" });
        SERIAL_PORT.write_str(" on ");
        SERIAL_PORT.write_str(if io == PRIMARY_IO { "primary" } else { "secondary" });
        SERIAL_PORT.write_str("): ");
        SERIAL_PORT.write_decimal((sectors / 2048) as u32);
        SERIAL_PORT.write_str(" MB");
        if lba48 { SERIAL_PORT.write_str(" LBA48"); }
        SERIAL_PORT.write_str(" model=");
        if let Ok(s) = core::str::from_utf8(&model[..end]) {
            SERIAL_PORT.write_str(s);
        }
        SERIAL_PORT.write_str("\n");

        DISKS[slot] = Some(AtaDisk { io_base: io, ctrl, slave, sectors, lba48, model });
    }
}

// ── Public init ───────────────────────────────────────────────────────────

/// Probe all four ATA positions and populate `DISKS[0..4]`.
/// Call once during boot after memory is set up.
pub unsafe fn init_all() {
    unsafe {
        probe_disk(0, PRIMARY_IO,   PRIMARY_CTRL,   false); // primary master
        probe_disk(1, PRIMARY_IO,   PRIMARY_CTRL,   true);  // primary slave
        probe_disk(2, SECONDARY_IO, SECONDARY_CTRL, false); // secondary master
        probe_disk(3, SECONDARY_IO, SECONDARY_CTRL, true);  // secondary slave
    }
}

// ── Backward-compatible wrappers (callers: main.rs, installer, terminal) ──

/// Legacy: init primary master only.
pub unsafe fn init() {
    unsafe { probe_disk(0, PRIMARY_IO, PRIMARY_CTRL, false); }
}
/// Legacy: init secondary master only.
pub unsafe fn init_secondary() {
    unsafe { probe_disk(2, SECONDARY_IO, SECONDARY_CTRL, false); }
}

pub fn is_present()     -> bool { unsafe { DISKS[0].is_some() } }
pub fn is_present_sec() -> bool { unsafe { DISKS[2].is_some() } }
pub fn sector_count()   -> u32  { unsafe { DISKS[0].as_ref().map(|d| d.sectors as u32).unwrap_or(0) } }
pub fn sector_count_sec() -> u32 { unsafe { DISKS[2].as_ref().map(|d| d.sectors as u32).unwrap_or(0) } }

// ── Unified read / write ──────────────────────────────────────────────────

/// Read one 512-byte sector from disk `idx` (0-3) at the given LBA.
pub unsafe fn read_sector(idx: usize, lba: u32, buf: &mut [u8; 512]) -> bool {
    let (io, ctrl, slave, lba48) = unsafe {
        match DISKS[idx].as_ref() {
            Some(d) => (d.io_base, d.ctrl, d.slave, d.lba48),
            None    => return false,
        }
    };
    unsafe { do_read(io, ctrl, slave, lba as u64, buf, lba48) }
}

/// Write one 512-byte sector to disk `idx` (0-3) at the given LBA.
pub unsafe fn write_sector(idx: usize, lba: u32, buf: &[u8; 512]) -> bool {
    let (io, ctrl, slave, lba48) = unsafe {
        match DISKS[idx].as_ref() {
            Some(d) => (d.io_base, d.ctrl, d.slave, d.lba48),
            None    => return false,
        }
    };
    unsafe { do_write(io, ctrl, slave, lba as u64, buf, lba48) }
}

// ── Legacy per-bus wrappers (installer.rs uses these) ─────────────────────

pub unsafe fn read_sector_sec(lba: u32, buf: &mut [u8; 512]) -> bool {
    unsafe { read_sector(2, lba, buf) }
}
pub unsafe fn write_sector_sec(lba: u32, buf: &[u8; 512]) -> bool {
    unsafe { write_sector(2, lba, buf) }
}

// ── Low-level PIO read/write ──────────────────────────────────────────────

unsafe fn select_drive(io: u16, ctrl: u16, slave: bool, lba28_bits: u8) {
    let base: u8 = if slave { 0xF0 } else { 0xE0 };
    unsafe { disk_outb(io, OFF_DRVHD, base | (lba28_bits & 0x0F)); }
    unsafe { delay400ns(ctrl); }
}

unsafe fn do_read(io: u16, ctrl: u16, slave: bool, lba: u64, buf: &mut [u8; 512], lba48: bool) -> bool {
    if !unsafe { wait_busy(io, 1_000_000) } { return false; }

    if lba48 {
        unsafe { disk_outb(io, OFF_DRVHD, if slave { 0x50 } else { 0x40 }); }
        unsafe { delay400ns(ctrl); }
        // HOB (high bytes first)
        unsafe {
            disk_outb(io, OFF_FEAT,  0);
            disk_outb(io, OFF_SECNT0, 0);            // sector count high byte
            disk_outb(io, OFF_LBA0,  ((lba >> 24) & 0xFF) as u8);
            disk_outb(io, OFF_LBA1,  ((lba >> 32) & 0xFF) as u8);
            disk_outb(io, OFF_LBA2,  ((lba >> 40) & 0xFF) as u8);
            // Low bytes
            disk_outb(io, OFF_SECNT0, 1);
            disk_outb(io, OFF_LBA0,  ( lba        & 0xFF) as u8);
            disk_outb(io, OFF_LBA1,  ((lba >>  8) & 0xFF) as u8);
            disk_outb(io, OFF_LBA2,  ((lba >> 16) & 0xFF) as u8);
            disk_outb(io, OFF_CMD, CMD_READ48);
        }
    } else {
        unsafe { select_drive(io, ctrl, slave, ((lba >> 24) & 0xF) as u8); }
        unsafe {
            disk_outb(io, OFF_FEAT,  0);
            disk_outb(io, OFF_SECNT0, 1);
            disk_outb(io, OFF_LBA0,  ( lba        & 0xFF) as u8);
            disk_outb(io, OFF_LBA1,  ((lba >>  8) & 0xFF) as u8);
            disk_outb(io, OFF_LBA2,  ((lba >> 16) & 0xFF) as u8);
            disk_outb(io, OFF_CMD, CMD_READ);
        }
    }

    unsafe { delay400ns(ctrl); }
    if !unsafe { wait_busy(io, 1_000_000) } { return false; }
    if !unsafe { wait_drq(io,  1_000_000) } { return false; }

    for i in 0..256usize {
        let w = unsafe { disk_inw(io, OFF_DATA) };
        buf[i * 2]     = (w & 0xFF) as u8;
        buf[i * 2 + 1] = (w >> 8)   as u8;
    }
    true
}

unsafe fn do_write(io: u16, ctrl: u16, slave: bool, lba: u64, buf: &[u8; 512], lba48: bool) -> bool {
    if !unsafe { wait_busy(io, 1_000_000) } { return false; }

    if lba48 {
        unsafe { disk_outb(io, OFF_DRVHD, if slave { 0x50 } else { 0x40 }); }
        unsafe { delay400ns(ctrl); }
        unsafe {
            disk_outb(io, OFF_FEAT,  0);
            disk_outb(io, OFF_SECNT0, 0);
            disk_outb(io, OFF_LBA0,  ((lba >> 24) & 0xFF) as u8);
            disk_outb(io, OFF_LBA1,  ((lba >> 32) & 0xFF) as u8);
            disk_outb(io, OFF_LBA2,  ((lba >> 40) & 0xFF) as u8);
            disk_outb(io, OFF_SECNT0, 1);
            disk_outb(io, OFF_LBA0,  ( lba        & 0xFF) as u8);
            disk_outb(io, OFF_LBA1,  ((lba >>  8) & 0xFF) as u8);
            disk_outb(io, OFF_LBA2,  ((lba >> 16) & 0xFF) as u8);
            disk_outb(io, OFF_CMD, CMD_WRITE48);
        }
    } else {
        unsafe { select_drive(io, ctrl, slave, ((lba >> 24) & 0xF) as u8); }
        unsafe {
            disk_outb(io, OFF_FEAT,  0);
            disk_outb(io, OFF_SECNT0, 1);
            disk_outb(io, OFF_LBA0,  ( lba        & 0xFF) as u8);
            disk_outb(io, OFF_LBA1,  ((lba >>  8) & 0xFF) as u8);
            disk_outb(io, OFF_LBA2,  ((lba >> 16) & 0xFF) as u8);
            disk_outb(io, OFF_CMD, CMD_WRITE);
        }
    }

    unsafe { delay400ns(ctrl); }
    if !unsafe { wait_drq(io, 1_000_000) } { return false; }

    for i in 0..256usize {
        let lo = buf[i * 2]     as u16;
        let hi = buf[i * 2 + 1] as u16;
        unsafe { disk_outw(io, OFF_DATA, lo | (hi << 8)); }
    }

    let flush = if lba48 { CMD_FLUSH48 } else { CMD_FLUSH };
    unsafe { disk_outb(io, OFF_CMD, flush); }
    unsafe { wait_busy(io, 1_000_000) }
}

// ── Convenience: multi-sector read/write ─────────────────────────────────

/// Read `count` consecutive sectors from disk `idx` into `buf`.
/// `buf` must be at least `count * 512` bytes.
pub unsafe fn read_sectors(idx: usize, lba: u32, count: u32, buf: &mut [u8]) -> bool {
    for i in 0..count {
        let offset = (i as usize) * 512;
        let chunk = unsafe { &mut *(buf[offset..offset + 512].as_mut_ptr() as *mut [u8; 512]) };
        if !unsafe { read_sector(idx, lba + i, chunk) } { return false; }
    }
    true
}

/// Write `count` consecutive sectors to disk `idx` from `buf`.
pub unsafe fn write_sectors(idx: usize, lba: u32, count: u32, buf: &[u8]) -> bool {
    for i in 0..count {
        let offset = (i as usize) * 512;
        let chunk = unsafe { &*(buf[offset..offset + 512].as_ptr() as *const [u8; 512]) };
        if !unsafe { write_sector(idx, lba + i, chunk) } { return false; }
    }
    true
}

/// Returns `true` if disk `idx` (0-3) was detected.
pub fn is_present_at(idx: usize) -> bool {
    if idx >= 4 { return false; }
    unsafe { DISKS[idx].is_some() }
}

/// Returns how many disks were detected across all four positions.
pub fn disk_count() -> usize {
    unsafe { DISKS.iter().filter(|d| d.is_some()).count() }
}

/// Returns a human-readable description of disk `idx`.
pub fn disk_info(idx: usize) -> Option<(u64, bool, bool)> {
    unsafe { DISKS[idx].as_ref().map(|d| (d.sectors, d.slave, d.lba48)) }
}
