#![no_std]
#![no_main]


mod mem;
mod multiboot;
mod vga_buffer;

use core::panic::PanicInfo;
use core::arch::asm;


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
    // Initialize VGA buffer
    vga_buffer::init();

    // Verify Multiboot2 magic number
    if magic != 0x36d76289 {
        println!("Magic: 0x{:08x}, Info: 0x{:08x}", magic, info_ptr);
        loop {}
    }
    println!("OK");
    println!("Welcome to India !");

    loop {}
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { loop {} }
