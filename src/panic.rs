//! Kernel Panic Handler
//! 
//! This module handles kernel panics - critical errors that require
//! the system to halt. It provides detailed error reporting and
//! ensures the system fails safely.

use core::panic::PanicInfo;
use core::arch::asm;
use crate::kernel::loggers::LOGGER;
use crate::kernel::serial::SERIAL_PORT;

/// Kernel panic handler - called when the kernel encounters a fatal error
/// 
/// This function:
/// 1. Disables interrupts to prevent further corruption
/// 2. Logs detailed panic information 
/// 3. Halts the system in a safe state
/// 4. Never returns (marked with `!`)
#[panic_handler]
pub fn panic_handler(info: &PanicInfo) -> ! {
    // Immediately disable interrupts to prevent further damage
    unsafe {
        asm!("cli", options(nostack, nomem));
    }
    
    unsafe {
        // Print panic header
        SERIAL_PORT.write_str("\n");
        SERIAL_PORT.write_str("=====================================\n");
        SERIAL_PORT.write_str("       KERNEL PANIC OCCURRED!       \n");
        SERIAL_PORT.write_str("=====================================\n");
        
        // Log through both serial and logger if available
        LOGGER.error("KERNEL PANIC - SYSTEM HALTING");
        
        // Print location information if available
        if let Some(location) = info.location() {
            SERIAL_PORT.write_str("Panic Location:\n");
            SERIAL_PORT.write_str("  File: ");
            SERIAL_PORT.write_str(location.file());
            SERIAL_PORT.write_str("\n  Line: ");
            SERIAL_PORT.write_decimal(location.line());
            SERIAL_PORT.write_str("\n  Column: ");
            SERIAL_PORT.write_decimal(location.column());
            SERIAL_PORT.write_str("\n");
        } else {
            SERIAL_PORT.write_str("Panic Location: Unknown\n");
        }
        
        // Print panic message - info.message() returns PanicMessage directly, not Option
        SERIAL_PORT.write_str("Panic Message: ");
        let _message = info.message();
        // TODO: Implement Display trait for better message formatting
        // For now, just indicate that a message exists
        SERIAL_PORT.write_str("(message available but formatting not implemented)\n");
        
        // TODO: Add more debugging info
        // - Register dump
        // - Stack trace
        // - Memory state
        // - Recent kernel activity log
        
        SERIAL_PORT.write_str("\nSystem State:\n");
        SERIAL_PORT.write_str("  Interrupts: DISABLED\n");
        SERIAL_PORT.write_str("  CPU: HALTED\n");
        SERIAL_PORT.write_str("  System: UNRECOVERABLE\n");
        
        SERIAL_PORT.write_str("\n");
        SERIAL_PORT.write_str("=====================================\n");
        SERIAL_PORT.write_str("System has been halted for safety.\n");
        SERIAL_PORT.write_str("Restart required.\n");
        SERIAL_PORT.write_str("=====================================\n");
        
        // Final log entry
        LOGGER.error("System halted due to kernel panic - restart required");
    }
    
    // Halt the CPU indefinitely
    // The HLT instruction stops CPU execution until an interrupt occurs,
    // but since we disabled interrupts, this effectively freezes the system
    unsafe {
        loop {
            asm!("hlt", options(nostack, nomem));
        }
    }
}

/// Enhanced panic function with custom message (for internal kernel use)
/// 
/// This allows kernel subsystems to trigger panics with specific context
pub fn kernel_panic(subsystem: &str, reason: &str) -> ! {
    unsafe {
        SERIAL_PORT.write_str("KERNEL PANIC in ");
        SERIAL_PORT.write_str(subsystem);
        SERIAL_PORT.write_str(": ");
        SERIAL_PORT.write_str(reason);
        SERIAL_PORT.write_str("\n");
    }
    
    panic!("Kernel subsystem failure: {}: {}", subsystem, reason);
}

/// Assert macro for kernel debugging (only in debug builds)
#[macro_export]
macro_rules! kernel_assert {
    ($condition:expr) => {
        if !($condition) {
            $crate::panic::kernel_panic("assertion", stringify!($condition));
        }
    };
    ($condition:expr, $message:expr) => {
        if !($condition) {
            $crate::panic::kernel_panic("assertion", $message);
        }
    };
}