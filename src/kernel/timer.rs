// src/kernel/timer.rs
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

pub fn init(frequency_hz: u32) {
    let divisor = 1193182 / frequency_hz; // PIT base frequency = 1.193182 MHz
    let divisor_low = (divisor & 0xFF) as u8;
    let divisor_high = ((divisor >> 8) & 0xFF) as u8;
    unsafe{
    SERIAL_PORT.write_str("  Timer init - Frequency: ");
    SERIAL_PORT.write_decimal(frequency_hz);
    SERIAL_PORT.write_str("Hz, Divisor: 0x");
    SERIAL_PORT.write_hex(divisor as u32);
    SERIAL_PORT.write_str("\n");
    }

    unsafe {
        asm!("out dx, al", in("dx") 0x43u16, in("al") 0x34u8); // Channel 0, mode 2, binary
        asm!("out dx, al", in("dx") 0x40u16, in("al") divisor_low);
        asm!("out dx, al", in("dx") 0x40u16, in("al") divisor_high);
    }
}

pub fn get_ticks() -> u64 {
    unsafe { crate::kernel::interrupts::TIMER_TICKS }
}