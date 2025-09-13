#![no_std]
#![no_main]
mod multiboot;
mod mem;
mod kernel;

use core::panic::PanicInfo;
use core::arch::asm;
use kernel::loggers;
use kernel::loggers::LOGGER;
use kernel::serial::SERIAL_PORT;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let magic: u32;
    let info_ptr: u32;

    // Read eax and ebx directly
    unsafe {
        asm!(
            "mov {0:e}, eax",
            "mov {1:e}, ebx",
            out(reg) magic,
            out(reg) info_ptr,
            options(nostack)
        );
    }
    unsafe{
        SERIAL_PORT.write_str("OS Loaded");
        panic!("This is a test panic!");
    }
    


    // Verify Multiboot2 magic number
    if magic != 0x36d76289 {
        loop {}
    }
    unsafe {
    }

    loop {}
}


#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        LOGGER.error("KERNEL PANIC OCCURRED!");
        
        if let Some(location) = info.location() {
            SERIAL_PORT.write_str("Location: ");
            SERIAL_PORT.write_str(location.file());
            SERIAL_PORT.write_str(":");
            SERIAL_PORT.write_decimal(location.line());
            SERIAL_PORT.write_str(":");
            SERIAL_PORT.write_decimal(location.column());
            SERIAL_PORT.write_str("\n");
        }
        
        // info.message() returns PanicMessage directly, not Option<PanicMessage>
        let msg = info.message();
        SERIAL_PORT.write_str("Message: ");
        SERIAL_PORT.write_str("(message formatting not implemented)\n");
        
        LOGGER.error("System halted due to panic");
    }
    
    // Disable interrupts and halt
    unsafe {
        asm!("cli");
        loop {
            asm!("hlt");
        }
    }
}
