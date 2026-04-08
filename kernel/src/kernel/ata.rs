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
    // Software reset
    unsafe { outb(PORT_CTRL, 0x04); }
    unsafe { outb(PORT_CTRL, 0x00); }
    if !unsafe { wait_not_busy() } {
        SERIAL_PORT.write_str("ATA: reset timeout\n");
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

    // Read 256 words = 512 bytes
    let words = unsafe {
        core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, 256)
    };
    for w in words.iter_mut() { *w = unsafe { inw(PORT_DATA) }; }
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

    let words = unsafe {
        core::slice::from_raw_parts(buf.as_ptr() as *const u16, 256)
    };
    for w in words.iter() { unsafe { outw(PORT_DATA, *w); } }

    // Flush write cache
    unsafe { outb(PORT_STATCMD, CMD_FLUSH); }
    unsafe { wait_not_busy() }
}
