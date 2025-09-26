//! Enhanced Kernel Panic Handler
//! 
//! This module handles kernel panics with proper message formatting
//! and detailed error reporting.

use core::panic::PanicInfo;
use core::arch::asm;
use core::fmt::Write;
use crate::kernel::loggers::LOGGER;
use crate::kernel::serial::SERIAL_PORT;

/// Kernel panic handler - called when the kernel encounters a fatal error
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
        
        // Print panic message using write_fmt
        SERIAL_PORT.write_str("Panic Message: ");
        let message = info.message();
        // Use the write_fmt method you already have in SerialPort
        SERIAL_PORT.write_fmt(format_args!("{}", message));
        SERIAL_PORT.write_str("\n");
        
        // Additional panic payload information (if any)
        if let Some(payload) = info.payload().downcast_ref::<&str>() {
            SERIAL_PORT.write_str("Payload: ");
            SERIAL_PORT.write_str(payload);
            SERIAL_PORT.write_str("\n");
        }
        
        // TODO: Add more debugging info
        // - Register dump
        // - Stack trace  
        // - Memory state
        // - Recent kernel activity log
        print_register_dump();
        
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
    halt_system();
}

/// Print basic CPU register dump for debugging
unsafe fn print_register_dump() {
    SERIAL_PORT.write_str("\nRegister Dump:\n");
    
    // For x86_64, capture registers in smaller batches to avoid running out of registers
    #[cfg(target_arch = "x86_64")]
    {
        // Batch 1: General purpose registers
        let rax: u64;
        let rbx: u64;
        let rcx: u64;
        let rdx: u64;
        
        asm!(
            "mov {rax}, rax",
            "mov {rbx}, rbx", 
            "mov {rcx}, rcx",
            "mov {rdx}, rdx",
            rax = out(reg) rax,
            rbx = out(reg) rbx,
            rcx = out(reg) rcx,
            rdx = out(reg) rdx,
            options(nostack, nomem)
        );
        
        SERIAL_PORT.write_str("  RAX: 0x");
        print_hex64(rax);
        SERIAL_PORT.write_str("  RBX: 0x");
        print_hex64(rbx);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("  RCX: 0x");
        print_hex64(rcx);
        SERIAL_PORT.write_str("  RDX: 0x");
        print_hex64(rdx);
        SERIAL_PORT.write_str("\n");
        
        // Batch 2: Stack and base pointers
        let rsp: u64;
        let rbp: u64;
        
        asm!(
            "mov {rsp}, rsp",
            "mov {rbp}, rbp",
            rsp = out(reg) rsp,
            rbp = out(reg) rbp,
            options(nostack, nomem)
        );
        
        SERIAL_PORT.write_str("  RSP: 0x");
        print_hex64(rsp);
        SERIAL_PORT.write_str("  RBP: 0x");
        print_hex64(rbp);
        SERIAL_PORT.write_str("\n");
        
        // Batch 3: Index registers
        let rsi: u64;
        let rdi: u64;
        
        asm!(
            "mov {rsi}, rsi",
            "mov {rdi}, rdi",
            rsi = out(reg) rsi,
            rdi = out(reg) rdi,
            options(nostack, nomem)
        );
        
        SERIAL_PORT.write_str("  RSI: 0x");
        print_hex64(rsi);
        SERIAL_PORT.write_str("  RDI: 0x");
        print_hex64(rdi);
        SERIAL_PORT.write_str("\n");
    }
    
    #[cfg(target_arch = "x86")]
    {
        // For 32-bit, also use smaller batches
        let eax: u32;
        let ebx: u32;
        let ecx: u32;
        let edx: u32;
        
        asm!(
            "mov {eax}, eax",
            "mov {ebx}, ebx", 
            "mov {ecx}, ecx",
            "mov {edx}, edx",
            eax = out(reg) eax,
            ebx = out(reg) ebx,
            ecx = out(reg) ecx,
            edx = out(reg) edx,
            options(nostack, nomem)
        );
        
        SERIAL_PORT.write_str("  EAX: 0x");
        SERIAL_PORT.write_hex(eax);
        SERIAL_PORT.write_str("  EBX: 0x");
        SERIAL_PORT.write_hex(ebx);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("  ECX: 0x");
        SERIAL_PORT.write_hex(ecx);
        SERIAL_PORT.write_str("  EDX: 0x");
        SERIAL_PORT.write_hex(edx);
        SERIAL_PORT.write_str("\n");
    }
}

/// Helper to print 64-bit hex values
unsafe fn print_hex64(mut value: u64) {
    if value == 0 {
        SERIAL_PORT.write_str("0000000000000000");
        return;
    }
    
    let mut digits = [0u8; 16];
    let mut i = 0;
    
    // Convert to hex, pad to 16 digits
    for _ in 0..16 {
        let digit = (value & 0xF) as u8;
        digits[i] = if digit < 10 {
            b'0' + digit
        } else {
            b'A' + (digit - 10)
        };
        value >>= 4;
        i += 1;
    }
    
    // Write in reverse order (most significant first)
    for j in (0..16).rev() {
        SERIAL_PORT.write_byte(digits[j]);
    }
}

/// Halt the system safely
fn halt_system() -> ! {
    unsafe {
        loop {
            asm!("hlt", options(nostack, nomem));
        }
    }
}

/// Enhanced panic function with custom message (for internal kernel use)
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

/// Panic with formatted message (using your write_fmt capability)
pub fn kernel_panic_fmt(subsystem: &str, args: core::fmt::Arguments) -> ! {
    unsafe {
        SERIAL_PORT.write_str("KERNEL PANIC in ");
        SERIAL_PORT.write_str(subsystem);
        SERIAL_PORT.write_str(": ");
        SERIAL_PORT.write_fmt(args);
        SERIAL_PORT.write_str("\n");
    }
    
    panic!("Kernel subsystem failure in {}", subsystem);
}

/// Enhanced assert macro for kernel debugging
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
    ($condition:expr, $($args:tt)*) => {
        if !($condition) {
            $crate::panic::kernel_panic_fmt("assertion", format_args!($($args)*));
        }
    };
}

/// Convenience macro for formatted kernel panics
#[macro_export]
macro_rules! kernel_panic {
    ($subsystem:expr, $($args:tt)*) => {
        $crate::panic::kernel_panic_fmt($subsystem, format_args!($($args)*))
    };
}