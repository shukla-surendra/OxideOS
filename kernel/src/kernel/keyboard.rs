use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

/// Handle keyboard interrupt (IRQ1)
pub unsafe fn handle_keyboard_interrupt() {
    // Check status register FIRST to see if this is actually keyboard data
    let status: u8;
    asm!("in al, 0x64", out("al") status, options(nostack, nomem));

    // Check if data is available (bit 0) and if it's from keyboard (bit 5 clear)
    if (status & 0x01) != 0 {
        if (status & 0x20) == 0 {
            // This is keyboard data - process it
            let scancode: u8;
            asm!("in al, 0x60", out("al") scancode, options(nostack, nomem));

            SERIAL_PORT.write_str("K64:0x");
            SERIAL_PORT.write_hex(scancode as u32);
            SERIAL_PORT.write_str(" ");

            // Basic key processing (just for demonstration)
            match scancode {
                0x1E => SERIAL_PORT.write_str("(A) "), // A key
                0x30 => SERIAL_PORT.write_str("(B) "), // B key
                0x2E => SERIAL_PORT.write_str("(C) "), // C key
                0x01 => SERIAL_PORT.write_str("(ESC) "), // Escape
                0x1C => SERIAL_PORT.write_str("(ENTER) "), // Enter
                _ => {} // Other keys
            }
        } else {
            // This is mouse data that arrived on keyboard IRQ - DON'T consume it!
            SERIAL_PORT.write_str("KEYBOARD IRQ but mouse data - not consuming\n");
            // Don't read from 0x60 - let the mouse handler get it
        }
    } else {
        SERIAL_PORT.write_str("KEYBOARD IRQ but no data available\n");
    }
}