// src/kernel/timer.rs - 64-bit timer implementation
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::interrupts::TIMER_TICKS;
use core::arch::asm;

pub unsafe fn init(freq_hz: u32) {
    // PIT base frequency is ~1.193182 MHz (same in 64-bit)
    let pit_freq = 1_193_182;
    let divisor = pit_freq / freq_hz;
    
    // Validate divisor
    if divisor > 0xFFFF {
        SERIAL_PORT.write_str("64-bit Timer init - ERROR: Divisor too large: 0x");
        SERIAL_PORT.write_hex(divisor);
        SERIAL_PORT.write_str("\n");
        return;
    }
    
    // Program PIT (Channel 0, Mode 2, Rate Generator)
    let divisor_low = (divisor & 0xFF) as u8;
    let divisor_high = ((divisor >> 8) & 0xFF) as u8;
    
    SERIAL_PORT.write_str("64-bit Timer init - Frequency: ");
    SERIAL_PORT.write_decimal(freq_hz);
    SERIAL_PORT.write_str("Hz, Divisor: 0x");
    SERIAL_PORT.write_hex(divisor);
    SERIAL_PORT.write_str("\n");
    
    // Send command: Channel 0, Lo/Hi byte, Mode 2, Binary
    // Note: Port I/O instructions work the same in 64-bit mode
    asm!(
        "out dx, al", 
        in("dx") 0x43u16, 
        in("al") 0x34u8,
        options(nostack, nomem)
    );
    
    // Send divisor low byte
    asm!(
        "out dx, al", 
        in("dx") 0x40u16, 
        in("al") divisor_low,
        options(nostack, nomem)
    );
    
    // Send divisor high byte
    asm!(
        "out dx, al", 
        in("dx") 0x40u16, 
        in("al") divisor_high,
        options(nostack, nomem)
    );
    
    SERIAL_PORT.write_str("64-bit PIT programmed - Command: 0x34, Divisor Low: 0x");
    SERIAL_PORT.write_hex(divisor_low as u32);
    SERIAL_PORT.write_str(", High: 0x");
    SERIAL_PORT.write_hex(divisor_high as u32);
    SERIAL_PORT.write_str("\n");
}

pub unsafe fn get_ticks() -> u64 {
    TIMER_TICKS
}

// Additional 64-bit specific timer functions

/// Get uptime in milliseconds (assuming 100Hz timer)
pub unsafe fn get_uptime_ms() -> u64 {
    TIMER_TICKS * 10  // 100Hz = 10ms per tick
}

/// Get uptime in seconds
pub unsafe fn get_uptime_seconds() -> u64 {
    TIMER_TICKS / 100  // 100Hz = 100 ticks per second
}

/// Sleep for approximately the specified number of ticks
/// Note: This is a busy-wait sleep - not suitable for production
pub unsafe fn sleep_ticks(ticks: u64) {
    let start = TIMER_TICKS;
    while (TIMER_TICKS - start) < ticks {
        asm!("pause", options(nostack, nomem)); // CPU hint for spin-wait loops
    }
}

/// High precision timer using RDTSC (Read Time-Stamp Counter)
/// Returns CPU cycles since reset
pub unsafe fn rdtsc() -> u64 {
    let low: u32;
    let high: u32;
    asm!(
        "rdtsc",
        out("eax") low,
        out("edx") high,
        options(nostack, nomem)
    );
    ((high as u64) << 32) | (low as u64)
}