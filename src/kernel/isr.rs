// Simplified isr.rs - timer interrupt bypasses this entirely

#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};
use crate::kernel::serial::SERIAL_PORT;

static TICKS: AtomicU32 = AtomicU32::new(0);
static EXCEPTION_COUNT: AtomicU32 = AtomicU32::new(0);
static KEYBOARD_COUNT: AtomicU32 = AtomicU32::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn debug_log_entry(marker: u32) {
    // Disable all debug logging for now
}

#[unsafe(no_mangle)]
pub extern "C" fn debug_log_gpf_error_code(error_code: u32) {
    unsafe {
        SERIAL_PORT.write_str("GPF: 0x");
        SERIAL_PORT.write_hex(error_code);
        SERIAL_PORT.write_str("\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn isr_dispatch(vec: u32) {
    match vec {
        32 => {
            // Timer interrupt - this should NOT be called now since we handle it in assembly
            unsafe {
                SERIAL_PORT.write_str("UNEXPECTED TIMER CALL TO RUST!\n");
            }
        },
        
        33 => {
            // Keyboard interrupt
            let count = KEYBOARD_COUNT.fetch_add(1, Ordering::Relaxed);
            
            unsafe {
                SERIAL_PORT.write_str("KB");
                SERIAL_PORT.write_decimal(count);
                SERIAL_PORT.write_str(": ");
                
                // Read keyboard scancode
                let scancode = super::ports::inb(0x60);
                SERIAL_PORT.write_str("0x");
                SERIAL_PORT.write_hex(scancode as u32);
                SERIAL_PORT.write_str("\n");
                
                // Send EOI
                super::pic::send_eoi(1);
            }
        },
        
        13 => {
            let count = EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);
            
            unsafe {
                SERIAL_PORT.write_str("GPF #");
                SERIAL_PORT.write_decimal(count);
                SERIAL_PORT.write_str("\n");
            }
            
            if count >= 3 {
                unsafe {
                    SERIAL_PORT.write_str("Multiple GPFs - halting\n");
                    core::arch::asm!("cli");
                    loop { core::arch::asm!("hlt"); }
                }
            }
        },
        
        8 => {
            unsafe {
                SERIAL_PORT.write_str("DOUBLE FAULT!\n");
                core::arch::asm!("cli");
                loop { core::arch::asm!("hlt"); }
            }
        },
        
        _ => {
            unsafe {
                SERIAL_PORT.write_str("INT");
                SERIAL_PORT.write_decimal(vec);
                SERIAL_PORT.write_str("\n");
                
                if vec >= 32 && vec <= 47 {
                    super::pic::send_eoi((vec - 32) as u8);
                }
            }
        }
    }
}

pub fn get_ticks() -> u32 {
    TICKS.load(Ordering::Relaxed)
}