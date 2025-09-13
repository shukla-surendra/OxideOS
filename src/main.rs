#![no_std]
#![no_main]
mod multiboot;
mod multiboot_parser;
mod framebuffer_draw;
mod mem;
mod kernel;

use core::panic::PanicInfo;
use core::arch::asm;
use kernel::loggers;
use kernel::loggers::LOGGER;
use kernel::serial::SERIAL_PORT;
use multiboot_parser::find_framebuffer;


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
        //initialize frame buffer
        // multiboot_parser::parse_multiboot(info_ptr).unwrap();
    }

    // Verify Multiboot2 magic number
    if magic != 0x36d76289 {
        unsafe{
            SERIAL_PORT.write_str("magic not found ! panicking !");
            panic!("This is panic from multiboot 2 Magic checker!");
        }
        loop {}
    }
    
// call find_framebuffer (mbi pointer as u32)
let fb_opt = unsafe { multiboot_parser::find_framebuffer(info_ptr) };

if let Some(fb) = fb_opt {
    unsafe {
        if fb.bpp == 32 {
            fb.draw_gradient();
            fb.fill_rect(20, 20, fb.width - 40, fb.height - 40, 0xFF_00_80_00);
            fb.draw_line(0, 0, (fb.width-1) as isize, (fb.height-1) as isize, 0xFF_FF_00_00);
            fb.draw_line((fb.width-1) as isize, 0, 0, (fb.height-1) as isize, 0xFF_00_FF_00);
        } else {
            fb.clear_32(0xFF_20_20_40);
        }
    }
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
