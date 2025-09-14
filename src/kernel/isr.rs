// Enhanced isr.rs with detailed debugging

#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::loggers::LOGGER;

static TICKS: AtomicU32 = AtomicU32::new(0);
static EXCEPTION_COUNT: AtomicU32 = AtomicU32::new(0);
static DEBUG_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Debug function called from assembly - logs entry points
#[unsafe(no_mangle)]
pub extern "C" fn debug_log_entry(marker: u32) {
    let count = DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
    unsafe {
        SERIAL_PORT.write_str("DEBUG[");
        SERIAL_PORT.write_decimal(count);
        SERIAL_PORT.write_str("]: Marker 0x");
        SERIAL_PORT.write_hex(marker);
        SERIAL_PORT.write_str("\n");
    }
}

/// Debug function to log GPF error codes
#[unsafe(no_mangle)]
pub extern "C" fn debug_log_gpf_error_code(error_code: u32) {
    unsafe {
        SERIAL_PORT.write_str("GPF Error Code: 0x");
        SERIAL_PORT.write_hex(error_code);
        
        // Decode error code bits
        if error_code & 1 != 0 {
            SERIAL_PORT.write_str(" (External)");
        } else {
            SERIAL_PORT.write_str(" (Internal)");
        }
        
        if error_code & 2 != 0 {
            SERIAL_PORT.write_str(" (IDT)");
        } else if error_code & 4 != 0 {
            SERIAL_PORT.write_str(" (LDT)");
        } else {
            SERIAL_PORT.write_str(" (GDT)");
        }
        
        let selector = error_code >> 3;
        SERIAL_PORT.write_str(" Selector: 0x");
        SERIAL_PORT.write_hex(selector);
        SERIAL_PORT.write_str("\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn isr_dispatch(vec: u32) {
    // Log every interrupt for debugging
    unsafe {
        SERIAL_PORT.write_str("ISR_DISPATCH: Vector ");
        SERIAL_PORT.write_decimal(vec);
        SERIAL_PORT.write_str("\n");
    }
    
    match vec {
        // Hardware IRQs (32-47)
        32..=47 => {
            let irq = (vec - 32) as u8;
            
            unsafe {
                SERIAL_PORT.write_str("Hardware IRQ ");
                SERIAL_PORT.write_decimal(irq as u32);
                SERIAL_PORT.write_str(" processing\n");
            }
            
            match irq {
                0 => {
                    // Timer IRQ0
                    let ticks = TICKS.fetch_add(1, Ordering::Relaxed);
                    
                    unsafe {
                        SERIAL_PORT.write_str("Timer tick: ");
                        SERIAL_PORT.write_decimal(ticks);
                        SERIAL_PORT.write_str("\n");
                    }
                },
                1 => {
                    unsafe {
                        SERIAL_PORT.write_str("Keyboard interrupt\n");
                    }
                },
                _ => {
                    unsafe {
                        SERIAL_PORT.write_str("Other hardware IRQ: ");
                        SERIAL_PORT.write_decimal(irq as u32);
                        SERIAL_PORT.write_str("\n");
                    }
                }
            }
            
            // Send EOI to PIC - this is critical!
            unsafe {
                SERIAL_PORT.write_str("Sending EOI for IRQ ");
                SERIAL_PORT.write_decimal(irq as u32);
                SERIAL_PORT.write_str("\n");
                
                super::pic::send_eoi(irq);
                
                SERIAL_PORT.write_str("EOI sent successfully\n");
            }
        },
        
        // CPU Exceptions
        13 => {
            let count = EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);
            
            unsafe {
                SERIAL_PORT.write_str("=== GPF #");
                SERIAL_PORT.write_decimal(count);
                SERIAL_PORT.write_str(" ===\n");
                
                // Don't use LOGGER in interrupt context - it might cause recursion
                SERIAL_PORT.write_str("GPF in interrupt context - likely causes:\n");
                SERIAL_PORT.write_str("- Invalid segment selector\n");
                SERIAL_PORT.write_str("- Stack issues\n");
                SERIAL_PORT.write_str("- IDT/GDT corruption\n");
            }
            
            if count >= 5 {
                unsafe {
                    SERIAL_PORT.write_str("Too many GPFs - system will halt after this\n");
                    
                    // Disable interrupts and halt
                    core::arch::asm!("cli");
                    loop {
                        core::arch::asm!("hlt");
                    }
                }
            }
        },
        
        8 => {
            unsafe {
                SERIAL_PORT.write_str("DOUBLE FAULT - SYSTEM CRITICAL!\n");
                core::arch::asm!("cli");
                loop {
                    core::arch::asm!("hlt");
                }
            }
        },
        
        _ => {
            unsafe {
                SERIAL_PORT.write_str("Unknown interrupt: ");
                SERIAL_PORT.write_decimal(vec);
                SERIAL_PORT.write_str("\n");
            }
        }
    }
    
    unsafe {
        SERIAL_PORT.write_str("ISR_DISPATCH: Returning from vector ");
        SERIAL_PORT.write_decimal(vec);
        SERIAL_PORT.write_str("\n");
    }
}

pub fn get_ticks() -> u32 {
    TICKS.load(Ordering::Relaxed)
}