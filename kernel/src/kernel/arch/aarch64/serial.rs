//! PL011 UART driver — QEMU `virt` board UART0 at physical 0x0900_0000.
//!
//! API mirrors `drivers/serial.rs` (the x86 16550 driver) so shared code —
//! panic handler, loggers, kmain — compiles unchanged on aarch64 via the
//! `crate::kernel::serial` re-export.
//!
//! The UART registers are MMIO.  Before `init()` runs we address them through
//! their physical address (works only if identity-mapped); `init()` switches
//! to the Limine HHDM mapping, which is the address every later access uses.

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

/// UART0 on the QEMU `virt` machine.
pub const UART0_PHYS: u64 = 0x0900_0000;

// PL011 register offsets.
const UARTDR: u64 = 0x00; // data
const UARTFR: u64 = 0x18; // flags
const UARTIBRD: u64 = 0x24; // integer baud divisor
const UARTFBRD: u64 = 0x28; // fractional baud divisor
const UARTLCR_H: u64 = 0x2C; // line control
const UARTCR: u64 = 0x30; // control
const UARTIMSC: u64 = 0x38; // interrupt mask
const UARTICR: u64 = 0x44; // interrupt clear

const FR_TXFF: u32 = 1 << 5; // transmit FIFO full
const FR_RXFE: u32 = 1 << 4; // receive FIFO empty

pub struct SerialPort {
    /// Virtual address of the UART register block.
    base: AtomicU64,
}

impl SerialPort {
    pub const fn new(phys_base: u64) -> Self {
        Self { base: AtomicU64::new(phys_base) }
    }

    fn reg(&self, offset: u64) -> *mut u32 {
        (self.base.load(Ordering::Relaxed) + offset) as *mut u32
    }

    /// Configure 8N1 + FIFO and enable TX/RX.  Re-bases the register block
    /// onto the Limine HHDM so MMIO works from the higher half.
    pub unsafe fn init(&self) {
        if let Some(resp) = crate::HHDM_REQUEST.get_response() {
            self.base.store(resp.offset() + UART0_PHYS, Ordering::Relaxed);
        }
        unsafe {
            self.reg(UARTCR).write_volatile(0); // disable while configuring
            self.reg(UARTICR).write_volatile(0x7FF); // clear pending interrupts
            self.reg(UARTIBRD).write_volatile(26); // 115200 baud @ 48 MHz clock
            self.reg(UARTFBRD).write_volatile(3);
            self.reg(UARTLCR_H).write_volatile((0b11 << 5) | (1 << 4)); // 8N1, FIFO on
            self.reg(UARTIMSC).write_volatile(0); // mask all UART interrupts
            self.reg(UARTCR).write_volatile((1 << 9) | (1 << 8) | 1); // RXE | TXE | EN
        }
    }

    /// Write a byte to the serial port
    pub unsafe fn write_byte(&self, byte: u8) {
        unsafe {
            while self.reg(UARTFR).read_volatile() & FR_TXFF != 0 {
                core::hint::spin_loop();
            }
            self.reg(UARTDR).write_volatile(byte as u32);
        }
    }

    /// Write a string to the serial port
    pub unsafe fn write_str(&self, s: &str) {
        unsafe {
            for byte in s.bytes() {
                self.write_byte(byte);
            }
        }
    }

    /// Write a formatted hex number (prints its own `0x` prefix)
    pub unsafe fn write_hex(&self, mut value: u32) {
        unsafe {
            self.write_str("0x");
            if value == 0 {
                self.write_byte(b'0');
                return;
            }
            let mut digits = [0u8; 8];
            let mut i = 0;
            while value > 0 && i < 8 {
                let digit = (value & 0xF) as u8;
                digits[i] = if digit < 10 { b'0' + digit } else { b'A' + (digit - 10) };
                value >>= 4;
                i += 1;
            }
            while i > 0 {
                i -= 1;
                self.write_byte(digits[i]);
            }
        }
    }

    /// Write a decimal number
    pub unsafe fn write_decimal(&self, mut value: u32) {
        unsafe {
            if value == 0 {
                self.write_byte(b'0');
                return;
            }
            let mut digits = [0u8; 10];
            let mut i = 0;
            while value > 0 && i < 10 {
                digits[i] = b'0' + (value % 10) as u8;
                value /= 10;
                i += 1;
            }
            while i > 0 {
                i -= 1;
                self.write_byte(digits[i]);
            }
        }
    }

    /// Read a byte from the serial port (if available)
    pub unsafe fn read_byte(&self) -> Option<u8> {
        unsafe {
            if self.reg(UARTFR).read_volatile() & FR_RXFE == 0 {
                Some((self.reg(UARTDR).read_volatile() & 0xFF) as u8)
            } else {
                None
            }
        }
    }

    /// core::fmt plumbing so `write_fmt` / `write!` work like on x86.
    pub unsafe fn write_fmt(&self, args: fmt::Arguments) {
        use fmt::Write;
        struct W<'a>(&'a SerialPort);
        impl fmt::Write for W<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                unsafe { self.0.write_str(s) };
                Ok(())
            }
        }
        let _ = W(self).write_fmt(args);
    }
}

pub static SERIAL_PORT: SerialPort = SerialPort::new(UART0_PHYS);
