// src/kernel/timer.rs
use crate::kernel::serial::SERIAL_PORT;
use core::arch::asm;

static mut TIMER_TICKS: u64 = 0;

pub unsafe fn init(freq_hz: u32) {
    // PIT base frequency is ~1.193182 MHz
    let pit_freq = 1_193_182;
    let divisor = pit_freq / freq_hz;
    
    // Validate divisor
    if divisor > 0xFFFF {
        SERIAL_PORT.write_str("Timer init - ERROR: Divisor too large: 0x");
        SERIAL_PORT.write_hex(divisor);
        SERIAL_PORT.write_str("\n");
        return;
    }
    
    // Program PIT (Channel 0, Mode 2, Rate Generator)
    let divisor_low = (divisor & 0xFF) as u8;
    let divisor_high = ((divisor >> 8) & 0xFF) as u8;
    
    SERIAL_PORT.write_str("Timer init - Frequency: ");
    SERIAL_PORT.write_decimal(freq_hz);
    SERIAL_PORT.write_str("Hz, Divisor: 0x");
    SERIAL_PORT.write_hex(divisor);
    SERIAL_PORT.write_str("\n");
    
    // Send command: Channel 0, Lo/Hi byte, Mode 2, Binary
    asm!("out dx, al", in("dx") 0x43u16, in("al") 0x34u8);
    // Send divisor
    asm!("out dx, al", in("dx") 0x40u16, in("al") divisor_low);
    asm!("out dx, al", in("dx") 0x40u16, in("al") divisor_high);
    
    SERIAL_PORT.write_str("  PIT programmed - Command: 0x34, Divisor Low: 0x");
    SERIAL_PORT.write_hex(divisor_low as u32);
    SERIAL_PORT.write_str(", High: 0x");
    SERIAL_PORT.write_hex(divisor_high as u32);
    SERIAL_PORT.write_str("\n");
}

pub unsafe fn get_ticks() -> u64 {
    TIMER_TICKS
}