// src/kernel/serial.rs
use core::arch::asm;
use core::fmt;
// COM1 serial port base address
const SERIAL_PORT_BASE: u16 = 0x3F8;

// Serial port registers (offsets from base)
const DATA_REG: u16 = 0;           // Data register (read/write)
const INT_ENABLE_REG: u16 = 1;     // Interrupt enable register
const FIFO_CTRL_REG: u16 = 2;      // FIFO control register
const LINE_CTRL_REG: u16 = 3;      // Line control register
const MODEM_CTRL_REG: u16 = 4;     // Modem control register
const LINE_STATUS_REG: u16 = 5;    // Line status register

// Line status register bits
const TRANSMIT_EMPTY: u8 = 1 << 5;  // Transmitter holding register empty
const DATA_READY: u8 = 1 << 0;      // Data ready

pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    pub const fn new(base: u16) -> Self {
        Self { base }
    }

    /// Initialize the serial port
    pub unsafe fn init(&self) {
        unsafe{
        // Disable interrupts
        self.outb(INT_ENABLE_REG, 0x00);

        // Enable DLAB (set baud rate divisor)
        self.outb(LINE_CTRL_REG, 0x80);

        // Set divisor to 3 (38400 baud)
        self.outb(DATA_REG, 0x03);     // Divisor low byte
        self.outb(INT_ENABLE_REG, 0x00); // Divisor high byte

        // 8 bits, no parity, one stop bit
        self.outb(LINE_CTRL_REG, 0x03);

        // Enable FIFO, clear them, with 14-byte threshold
        self.outb(FIFO_CTRL_REG, 0xC7);

        // IRQs enabled, RTS/DSR set
        self.outb(MODEM_CTRL_REG, 0x0B);

        // Test serial chip (send byte 0xAE and check if serial returns same byte)
        self.outb(MODEM_CTRL_REG, 0x1E);
        self.outb(DATA_REG, 0xAE);

        // Check if serial is faulty
        if self.inb(DATA_REG) != 0xAE {
            // Serial is faulty, but we'll continue anyway
        }

        // Set it in normal operation mode
        self.outb(MODEM_CTRL_REG, 0x0F);
    }
    }

    /// Write a byte to the serial port
    pub unsafe fn write_byte(&self, byte: u8) {
        unsafe{
        // Wait for transmit buffer to be empty
        while (self.inb(LINE_STATUS_REG) & TRANSMIT_EMPTY) == 0 {}
        // Send the byte
        self.outb(DATA_REG, byte);
        }
    }

    /// Write a string to the serial port
    pub unsafe fn write_str(&self, s: &str) {
        unsafe{
                for byte in s.bytes() {
                    self.write_byte(byte);
                }
        }
    }



    /// Write a formatted hex number (useful for debugging)
    pub unsafe fn write_hex(&self, mut value: u32) {
        unsafe { self.write_str("0x") };
        
        // Handle zero case
        if value == 0 {
            unsafe { self.write_byte(b'0') };
            return;
        }

        // Convert to hex string
        let mut digits = [0u8; 8]; // Max 8 hex digits for u32
        let mut i = 0;
        
        while value > 0 && i < 8 {
            let digit = (value & 0xF) as u8;
            digits[i] = if digit < 10 {
                b'0' + digit
            } else {
                b'A' + (digit - 10)
            };
            value >>= 4;
            i += 1;
        }
        
        // Write digits in reverse order
        while i > 0 {
            i -= 1;
            self.write_byte(digits[i]);
        }
    }

    /// Write a decimal number
    pub unsafe fn write_decimal(&self, mut value: u32) {
        if value == 0 {
            self.write_byte(b'0');
            return;
        }

        let mut digits = [0u8; 10]; // Max 10 digits for u32
        let mut i = 0;
        
        while value > 0 && i < 10 {
            digits[i] = b'0' + (value % 10) as u8;
            value /= 10;
            i += 1;
        }
        
        // Write digits in reverse order
        while i > 0 {
            i -= 1;
            self.write_byte(digits[i]);
        }
    }

    /// Read a byte from the serial port (if available)
    pub unsafe fn read_byte(&self) -> Option<u8> {
        if (self.inb(LINE_STATUS_REG) & DATA_READY) != 0 {
            Some(self.inb(DATA_REG))
        } else {
            None
        }
    }

    /// Low-level port I/O functions
    #[inline]
    unsafe fn outb(&self, reg: u16, value: u8) {
        let port = self.base + reg;
        asm!("out dx, al", in("dx") port, in("al") value, options(nostack, nomem));
    }

    #[inline]
    unsafe fn inb(&self, reg: u16) -> u8 {
        let port = self.base + reg;
        let value: u8;
        asm!("in al, dx", in("dx") port, out("al") value, options(nostack, nomem));
        value
    }

    /// Write formatted data (supports `format_args!`)
    /// Usage: SERIAL_PORT.write_fmt(format_args!("x = {:#x}\n", x));
    pub unsafe fn write_fmt(&self, args: fmt::Arguments) {
        // small wrapper that implements core::fmt::Write by forwarding to write_str
        struct W<'a> {
            port: &'a SerialPort,
        }

        impl<'a> fmt::Write for W<'a> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                // SAFETY: forwarding to existing write_str which uses port I/O
                unsafe { self.port.write_str(s) };
                Ok(())
            }
        }

        let mut w = W { port: self };
        // fmt::write will call W::write_str repeatedly with parts of the formatted output
        let _ = fmt::write(&mut w, args);
    }
}

// Global serial port instance
pub static SERIAL_PORT: SerialPort = SerialPort::new(SERIAL_PORT_BASE);

// Convenience macros for logging
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        unsafe {
            $crate::serial::SERIAL_PORT.write_str(&format_args!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! serial_println {
    () => {
        unsafe { $crate::serial::SERIAL_PORT.write_str("\n"); }
    };
    ($($arg:tt)*) => {{
        unsafe {
            $crate::serial::SERIAL_PORT.write_str(&format_args!($($arg)*));
            $crate::serial::SERIAL_PORT.write_str("\n");
        }
    }};
}