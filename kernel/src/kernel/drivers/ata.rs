//! ATA PIO driver — primary bus (I/O base 0x1F0), LBA28, master drive.
//!
//! Only basic polling I/O is implemented (no DMA, no IRQ).  Each sector is
//! 512 bytes.  Call `init()` once after interrupts are enabled to detect
//! whether a disk is attached.

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

// ── I/O port map (primary bus) ─────────────────────────────────────────────
const PORT_DATA:    u16 = 0x1F0;
const PORT_ERR:     u16 = 0x1F1; // write: features
const PORT_SECNT:   u16 = 0x1F2;
const PORT_LBA0:    u16 = 0x1F3;
const PORT_LBA1:    u16 = 0x1F4;
const PORT_LBA2:    u16 = 0x1F5;
const PORT_DRVHD:   u16 = 0x1F6;
const PORT_STATCMD: u16 = 0x1F7; // read: status, write: command
const PORT_CTRL:    u16 = 0x3F6; // device control / alt-status

// ── ATA commands ───────────────────────────────────────────────────────────
const CMD_IDENTIFY: u8 = 0xEC;
const CMD_READ:     u8 = 0x20;
const CMD_WRITE:    u8 = 0x30;
const CMD_FLUSH:    u8 = 0xE7;

// ── Status bits ────────────────────────────────────────────────────────────
const SR_ERR: u8 = 1 << 0;
const SR_DRQ: u8 = 1 << 3;
const SR_BSY: u8 = 1 << 7;

static mut DISK_PRESENT: bool  = false;
static mut DISK_SECTORS: u32   = 0;

// ── Port helpers ───────────────────────────────────────────────────────────

unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") v, options(nomem, nostack)); }
    v
}

unsafe fn inw(port: u16) -> u16 {
    let v: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") v, options(nomem, nostack)); }
    v
}

unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack)); }
}

unsafe fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack)); }
}

/// 400 ns delay — read alt-status 4 times.
unsafe fn io_delay() {
    for _ in 0..4 { let _ = unsafe { inb(PORT_CTRL) }; }
}

/// Wait until BSY clears.  Returns false on timeout.
unsafe fn wait_not_busy() -> bool {
    for _ in 0..1_000_000u32 {
        if unsafe { inb(PORT_STATCMD) } & SR_BSY == 0 { return true; }
    }
    false
}

/// Wait until DRQ is set (data ready).  Returns false on error/timeout.
unsafe fn wait_drq() -> bool {
    for _ in 0..1_000_000u32 {
        let s = unsafe { inb(PORT_STATCMD) };
        if s & SR_ERR != 0 { return false; }
        if s & SR_DRQ != 0 { return true;  }
    }
    false
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Initialise the ATA driver and detect the master disk on the primary bus.
///
/// Must be called after interrupts are enabled (the PIC should be set up).
pub unsafe fn init() {
    // Disable device IRQ, set nIEN, then do a soft reset.
    unsafe { outb(PORT_CTRL, 0x02); }   // nIEN – disable IRQ while we probe
    unsafe { outb(PORT_CTRL, 0x06); }   // SRST | nIEN
    // 5 µs hold: read alt-status several times as a delay
    for _ in 0..20 { let _ = unsafe { inb(PORT_CTRL) }; }
    unsafe { outb(PORT_CTRL, 0x02); }   // Clear SRST, keep nIEN

    // Wait up to ~500 ms for BSY to clear (10 M × ~50 ns each).
    let mut status_after_reset = 0u8;
    let mut found = false;
    for _ in 0..10_000_000u32 {
        status_after_reset = unsafe { inb(PORT_STATCMD) };
        if status_after_reset & SR_BSY == 0 { found = true; break; }
    }
    if !found {
        SERIAL_PORT.write_str("ATA: reset timeout, status=0x");
        SERIAL_PORT.write_hex(status_after_reset as u32);
        SERIAL_PORT.write_str("\n");
        return;
    }

    // Select master drive
    unsafe { outb(PORT_DRVHD, 0xA0); }
    unsafe { io_delay(); }

    // Floating bus check — 0xFF means no controller
    let status = unsafe { inb(PORT_STATCMD) };
    if status == 0xFF {
        SERIAL_PORT.write_str("ATA: no controller (FF)\n");
        return;
    }

    // Issue IDENTIFY
    unsafe { outb(PORT_SECNT, 0); }
    unsafe { outb(PORT_LBA0,  0); }
    unsafe { outb(PORT_LBA1,  0); }
    unsafe { outb(PORT_LBA2,  0); }
    unsafe { outb(PORT_STATCMD, CMD_IDENTIFY); }
    unsafe { io_delay(); }

    // Status == 0 means no drive
    let status = unsafe { inb(PORT_STATCMD) };
    if status == 0 {
        SERIAL_PORT.write_str("ATA: no disk\n");
        return;
    }

    if !unsafe { wait_not_busy() } {
        SERIAL_PORT.write_str("ATA: BSY timeout after IDENTIFY\n");
        return;
    }

    // Non-ATA device (ATAPI): LBA1/LBA2 will be non-zero
    if unsafe { inb(PORT_LBA1) } != 0 || unsafe { inb(PORT_LBA2) } != 0 {
        SERIAL_PORT.write_str("ATA: ATAPI or unknown device — skipping\n");
        return;
    }

    if !unsafe { wait_drq() } {
        SERIAL_PORT.write_str("ATA: DRQ timeout during IDENTIFY\n");
        return;
    }

    // Read the 256-word IDENTIFY response
    let mut id = [0u16; 256];
    for w in id.iter_mut() { *w = unsafe { inw(PORT_DATA) }; }

    // Words 60–61 contain the 28-bit LBA sector count
    DISK_SECTORS  = (id[60] as u32) | ((id[61] as u32) << 16);
    DISK_PRESENT  = true;

    SERIAL_PORT.write_str("ATA: disk detected, sectors=");
    SERIAL_PORT.write_decimal(DISK_SECTORS);
    SERIAL_PORT.write_str(" (~");
    SERIAL_PORT.write_decimal(DISK_SECTORS / 2048);
    SERIAL_PORT.write_str(" MB)\n");
}

/// Returns `true` when a disk was found during `init()`.
pub fn is_present() -> bool { unsafe { DISK_PRESENT } }

/// Number of 512-byte sectors on the disk (LBA28 count).
pub fn sector_count() -> u32 { unsafe { DISK_SECTORS } }

/// Read one 512-byte sector into `buf`.  Returns `false` on error.
pub unsafe fn read_sector(lba: u32, buf: &mut [u8; 512]) -> bool {
    if !unsafe { DISK_PRESENT } { return false; }
    if !unsafe { wait_not_busy() } { return false; }

    unsafe {
        outb(PORT_DRVHD,   0xE0 | ((lba >> 24) as u8 & 0x0F));
        outb(PORT_ERR,     0x00);
        outb(PORT_SECNT,   1);
        outb(PORT_LBA0,    (lba       & 0xFF) as u8);
        outb(PORT_LBA1,    (lba >> 8  & 0xFF) as u8);
        outb(PORT_LBA2,    (lba >> 16 & 0xFF) as u8);
        outb(PORT_STATCMD, CMD_READ);
        io_delay();
    }

    if !unsafe { wait_not_busy() } { return false; }
    if !unsafe { wait_drq()      } { return false; }

    // Read 256 words = 512 bytes.
    // Write via raw byte pointer to avoid alignment requirement on u16 slices.
    for i in 0..256usize {
        let word = unsafe { inw(PORT_DATA) };
        buf[i * 2]     = (word & 0xFF) as u8;
        buf[i * 2 + 1] = (word >> 8)   as u8;
    }
    true
}

/// Write one 512-byte sector from `buf`.  Returns `false` on error.
pub unsafe fn write_sector(lba: u32, buf: &[u8; 512]) -> bool {
    if !unsafe { DISK_PRESENT } { return false; }
    if !unsafe { wait_not_busy() } { return false; }

    unsafe {
        outb(PORT_DRVHD,   0xE0 | ((lba >> 24) as u8 & 0x0F));
        outb(PORT_ERR,     0x00);
        outb(PORT_SECNT,   1);
        outb(PORT_LBA0,    (lba       & 0xFF) as u8);
        outb(PORT_LBA1,    (lba >> 8  & 0xFF) as u8);
        outb(PORT_LBA2,    (lba >> 16 & 0xFF) as u8);
        outb(PORT_STATCMD, CMD_WRITE);
        io_delay();
    }

    if !unsafe { wait_drq() } { return false; }

    // Write 256 words = 512 bytes.
    // Read via byte pairs to avoid alignment requirement on u16 slices.
    for i in 0..256usize {
        let lo = buf[i * 2]     as u16;
        let hi = buf[i * 2 + 1] as u16;
        unsafe { outw(PORT_DATA, lo | (hi << 8)); }
    }

    // Flush write cache
    unsafe { outb(PORT_STATCMD, CMD_FLUSH); }
    unsafe { wait_not_busy() }
}

// ── Secondary bus (0x170) ─────────────────────────────────────────────────

const SEC_DATA:    u16 = 0x170;
const SEC_ERR:     u16 = 0x171;
const SEC_SECNT:   u16 = 0x172;
const SEC_LBA0:    u16 = 0x173;
const SEC_LBA1:    u16 = 0x174;
const SEC_LBA2:    u16 = 0x175;
const SEC_DRVHD:   u16 = 0x176;
const SEC_STATCMD: u16 = 0x177;
const SEC_CTRL:    u16 = 0x376;

static mut SEC_DISK_PRESENT: bool = false;
static mut SEC_DISK_SECTORS: u32  = 0;

unsafe fn sec_wait_not_busy() -> bool {
    for _ in 0..1_000_000u32 {
        if unsafe { inb(SEC_STATCMD) } & SR_BSY == 0 { return true; }
    }
    false
}

unsafe fn sec_wait_drq() -> bool {
    for _ in 0..1_000_000u32 {
        let s = unsafe { inb(SEC_STATCMD) };
        if s & SR_ERR != 0 { return false; }
        if s & SR_DRQ != 0 { return true;  }
    }
    false
}

/// Initialise the secondary ATA bus (0x170) and detect master drive.
/// Call after `init()`.
pub unsafe fn init_secondary() {
    // Disable IRQ, soft-reset secondary bus.
    unsafe {
        outb(SEC_CTRL, 0x02);
        outb(SEC_CTRL, 0x06);
        for _ in 0..20 { let _ = inb(SEC_CTRL); }
        outb(SEC_CTRL, 0x02);
    }

    // Wait for BSY to clear.
    let mut found = false;
    for _ in 0..10_000_000u32 {
        if unsafe { inb(SEC_STATCMD) } & SR_BSY == 0 { found = true; break; }
    }
    if !found {
        unsafe { SERIAL_PORT.write_str("ATA-sec: reset timeout\n"); }
        return;
    }

    // Select master on secondary.
    unsafe {
        outb(SEC_DRVHD, 0xA0);
        for _ in 0..4 { let _ = inb(SEC_CTRL); } // 400 ns delay
    }

    let status = unsafe { inb(SEC_STATCMD) };
    if status == 0xFF {
        unsafe { SERIAL_PORT.write_str("ATA-sec: no controller\n"); }
        return;
    }

    // IDENTIFY
    unsafe {
        outb(SEC_SECNT,   0);
        outb(SEC_LBA0,    0);
        outb(SEC_LBA1,    0);
        outb(SEC_LBA2,    0);
        outb(SEC_STATCMD, CMD_IDENTIFY);
        for _ in 0..4 { let _ = inb(SEC_CTRL); }
    }

    let status = unsafe { inb(SEC_STATCMD) };
    if status == 0 {
        unsafe { SERIAL_PORT.write_str("ATA-sec: no disk\n"); }
        return;
    }

    if !unsafe { sec_wait_not_busy() } {
        unsafe { SERIAL_PORT.write_str("ATA-sec: BSY timeout\n"); }
        return;
    }

    // ATAPI check
    if unsafe { inb(SEC_LBA1) } != 0 || unsafe { inb(SEC_LBA2) } != 0 {
        unsafe { SERIAL_PORT.write_str("ATA-sec: ATAPI — skip\n"); }
        return;
    }

    if !unsafe { sec_wait_drq() } {
        unsafe { SERIAL_PORT.write_str("ATA-sec: DRQ timeout\n"); }
        return;
    }

    let mut id = [0u16; 256];
    for w in id.iter_mut() { *w = unsafe { inw(SEC_DATA) }; }

    unsafe {
        SEC_DISK_SECTORS = (id[60] as u32) | ((id[61] as u32) << 16);
        SEC_DISK_PRESENT = true;
        SERIAL_PORT.write_str("ATA-sec: disk detected, sectors=");
        SERIAL_PORT.write_decimal(SEC_DISK_SECTORS);
        SERIAL_PORT.write_str("\n");
    }
}

/// Secondary disk presence.
pub fn is_present_sec() -> bool { unsafe { SEC_DISK_PRESENT } }

/// Number of 512-byte sectors on the secondary disk.
pub fn sector_count_sec() -> u32 { unsafe { SEC_DISK_SECTORS } }

/// Write one 512-byte sector to the secondary disk. Returns `false` on error.
pub unsafe fn write_sector_sec(lba: u32, buf: &[u8; 512]) -> bool {
    if !unsafe { SEC_DISK_PRESENT } { return false; }
    if !unsafe { sec_wait_not_busy() } { return false; }

    unsafe {
        outb(SEC_DRVHD,   0xE0 | ((lba >> 24) as u8 & 0x0F));
        outb(SEC_ERR,     0x00);
        outb(SEC_SECNT,   1);
        outb(SEC_LBA0,    (lba       & 0xFF) as u8);
        outb(SEC_LBA1,    (lba >> 8  & 0xFF) as u8);
        outb(SEC_LBA2,    (lba >> 16 & 0xFF) as u8);
        outb(SEC_STATCMD, CMD_WRITE);
        for _ in 0..4 { let _ = inb(SEC_CTRL); } // 400 ns
    }

    if !unsafe { sec_wait_drq() } { return false; }

    for i in 0..256usize {
        let lo = buf[i * 2]     as u16;
        let hi = buf[i * 2 + 1] as u16;
        unsafe { outw(SEC_DATA, lo | (hi << 8)); }
    }

    unsafe { outb(SEC_STATCMD, CMD_FLUSH); }
    unsafe { sec_wait_not_busy() }
}

/// Read one 512-byte sector from the secondary disk into `buf`.
pub unsafe fn read_sector_sec(lba: u32, buf: &mut [u8; 512]) -> bool {
    if !unsafe { SEC_DISK_PRESENT } { return false; }
    if !unsafe { sec_wait_not_busy() } { return false; }

    unsafe {
        outb(SEC_DRVHD,   0xE0 | ((lba >> 24) as u8 & 0x0F));
        outb(SEC_ERR,     0x00);
        outb(SEC_SECNT,   1);
        outb(SEC_LBA0,    (lba       & 0xFF) as u8);
        outb(SEC_LBA1,    (lba >> 8  & 0xFF) as u8);
        outb(SEC_LBA2,    (lba >> 16 & 0xFF) as u8);
        outb(SEC_STATCMD, CMD_READ);
        for _ in 0..4 { let _ = inb(SEC_CTRL); } // 400 ns
    }

    if !unsafe { sec_wait_not_busy() } { return false; }
    if !unsafe { sec_wait_drq()      } { return false; }

    for i in 0..256usize {
        let word = unsafe { inw(SEC_DATA) };
        buf[i * 2]     = (word & 0xFF) as u8;
        buf[i * 2 + 1] = (word >> 8)   as u8;
    }
    true
}
