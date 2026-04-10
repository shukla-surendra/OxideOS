//! System shutdown / reboot for OxideOS.
//!
//! Tries several power-off mechanisms in order:
//!   1. QEMU / Bochs ISA debug exit port (0x604 / 0xB004)
//!   2. VirtualBox ACPI control port (0x4004)
//!   3. ACPI S5 via the standard PM1a control port (0x404 — used by many firmwares)
//!   4. Fallback: disable interrupts and halt (appears as a freeze, but is safe)

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

/// Write a 16-bit value to an I/O port.
#[inline]
unsafe fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, nomem)); }
}

/// Write an 8-bit value to an I/O port.
#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nostack, nomem)); }
}

/// Power off the machine.  Never returns under normal circumstances.
pub fn poweroff() -> ! {
    unsafe {
        SERIAL_PORT.write_str("OxideOS: shutting down...\n");

        // 1. QEMU ISA debug / power-off device (iobase 0x604, value 0x2000)
        outw(0x604, 0x2000);

        // 2. Bochs power-off port
        outw(0xB004, 0x2000);

        // 3. VirtualBox ACPI PM1a control register
        //    SLP_TYPa = 0 (S5), SLP_EN = 1  →  value = 0x2000
        //    Port 0x4004 is the VirtualBox ACPI port.
        outw(0x4004, 0x3400);

        // 4. ACPI PM1a_CNT at 0x404 (common firmware default)
        outw(0x0404, 0x2000);

        // 5. APM BIOS power off (legacy fallback, mostly for real HW)
        //    Normally needs APM tables, but the INT 15h call below
        //    is a best-effort attempt:
        //      AX = 5307h (APM Set Power State), BX = 0001h (all devices),
        //      CX = 0003h (off) — ignored in protected/long mode but harmless.

        // 6. Final fallback: disable interrupts and halt every CPU.
        asm!("cli");
        loop { asm!("hlt", options(nostack, nomem)); }
    }
}

/// Reboot the machine via the keyboard controller (classic 8042 reset).
pub fn reboot() -> ! {
    unsafe {
        SERIAL_PORT.write_str("OxideOS: rebooting...\n");

        // Pulse the reset line via 8042 output port.
        // Wait for input buffer empty first.
        for _ in 0..0xFF_FFFFu32 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x02) == 0 { break; }
        }
        outb(0x64, 0xFE); // pulse reset

        // If that didn't work, try triple fault.
        asm!(
            "lidt [{idtr}]",
            "int3",
            idtr = in(reg) &[0u8; 10],
            options(nostack)
        );

        loop { asm!("hlt", options(nostack, nomem)); }
    }
}
