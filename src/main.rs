#![no_std]
#![no_main]


mod mem;
mod multiboot;
mod kernel;

use core::panic::PanicInfo;
// use multiboot::parse_multiboot;
use kernel::{console::Console, keyboard::read_scancode_nonblock};
use kernel::scancode::{decode_scancode, DecodedKey};

#[unsafe(no_mangle)]
pub extern "C" fn _start(magic: u32, info_ptr: u32) -> ! {
    // Verify Multiboot2 magic number
    // if magic != 0x36d76289 {
    //     // GRUB didn’t load us properly
    //     loop {}
    // }

    unsafe {
        // Just write "OK" to VGA to confirm
        let vga = 0xb8000 as *mut u8;
        *vga.offset(0) = b'O';
        *vga.offset(1) = 0x0f;
        *vga.offset(2) = b'K';
        *vga.offset(3) = 0x0f;

        // Now parse GRUB’s Multiboot2 info structure
        // parse_multiboot(info_ptr as usize);
    }

    loop {}
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { loop {} }
