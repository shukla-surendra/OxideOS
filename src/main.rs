#![no_std]
#![no_main]


mod mem;
mod multiboot;
mod vga_buffer;
mod multiboot2_parser;

use core::panic::PanicInfo;
use core::arch::asm;
use multiboot2_parser::{parse_multiboot, get_framebuffer_info, draw_rectangle};

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
    println!("Magic: 0x{:08x}, Info: 0x{:08x}", magic, info_ptr);
    println!("Welcome to OxideOS!");
    unsafe {
        parse_multiboot(info_ptr as usize);
        if let Some(fb_tag) = get_framebuffer_info() {
            println!("Using framebuffer in main: width {}, height {}", fb_tag.width, fb_tag.height);
            draw_rectangle(&fb_tag, 100, 100, 50, 50, 0xFF, 0xFF, 0x00); // Draw a blue pixel at (40, 40)
        } else {
            println!("No framebuffer info available");
        }
    }

    loop {}
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { loop {} }
