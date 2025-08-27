#![no_std]
#![no_main]

use core::panic::PanicInfo;

mod kernel;

use kernel::{console::Console, keyboard::read_scancode_nonblock};
use kernel::scancode::{decode_scancode, DecodedKey};

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut con = Console::new(0x1F);
    con.clear();
    con.write_str("Welcome to OxideOS!\nType on your keyboard...\n");

    loop {
        if let Some(sc) = read_scancode_nonblock() {
            match decode_scancode(sc) {
                DecodedKey::Ascii(b) => con.putc(b),
                DecodedKey::Enter    => con.newline(),
                DecodedKey::Backspace=> con.backspace(),
                DecodedKey::None     => { /* ignore */ }
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { loop {} }
