// Disable the Rust standard library, as we're building a bare-metal OS
// The standard library relies on an OS, but we're creating one!
#![no_std]

// Disable the default main function provided by Rust
// We'll define our own entry point for the kernel
#![no_main]

// Import the PanicInfo type from Rust's core library
// This is needed to handle errors (panics) in a no_std environment
use core::panic::PanicInfo;

// The entry point of our kernel, called by the bootloader
// `#[no_mangle]` ensures the function name remains "_start" for the bootloader to find
// `extern "C"` uses the C calling convention, common for OS kernels
// `-> !` means the function never returns (it loops forever)
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // The VGA text buffer is a memory region at address 0xb8000
    // It's used to display text on the screen in text mode
    // We cast the address to a mutable raw pointer to write to it
    let vga_buffer = 0xb8000 as *mut u8;

    // The message we want to display on the screen
    let message = b"Welcome to OxideOS!";

    // Loop through each character in the message
    // `enumerate` gives us the index (i) and the character (byte)
    for (i, &byte) in message.iter().enumerate() {
        // Writing to the VGA buffer requires unsafe code because we're
        // directly accessing memory, which Rust considers dangerous
        unsafe {
            // Each character in the VGA buffer takes 2 bytes:
            // 1. The ASCII character (e.g., 'W' for Welcome)
            // 2. The color attribute (0x0f = white text on black background)
            // We multiply the index by 2 to leave space for the color byte
            *vga_buffer.offset(i as isize * 2) = byte; // Write the character
            *vga_buffer.offset(i as isize * 2 + 1) = 0x0f; // Set color (white on black)
        }
        blue_screen();
    }

    // Keep the kernel running in an infinite loop
    // Without this, the CPU would try to execute random memory after _start
    loop {}
}
/// Clear the whole VGA text screen with blue background
fn blue_screen() {
    let vga = 0xb8000 as *mut u8;
    for i in 0..(80 * 25) {
        unsafe {
            *vga.add(i * 2) = b' ';      // Space character
            *vga.add(i * 2 + 1) = 0x10;  // Black text (0x0) on Blue background (0x1 << 4)
        }
    }
}

/// Fill the 80×25 VGA text screen with spaces using the given color (e.g., 0x0f = white on black).
fn clear_screen(color: u8) {
    let vga = 0xb8000 as *mut u8;             // Base address of VGA text buffer
    for i in 0..(80 * 25) {                    // 80 columns × 25 rows = 2000 cells
        unsafe {
            *vga.add(i * 2) = b' ';            // Byte 0: ASCII space
            *vga.add(i * 2 + 1) = color;       // Byte 1: attribute (fg/bg color)
        }
    }
}


// Define a panic handler for when something goes wrong
// In a no_std environment, we must provide this ourselves
// `!` means it never returns, as we just loop forever
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // In a real OS, you might display an error message
    // For simplicity, we just loop forever if a panic occurs
    loop {}
}