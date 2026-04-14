//! pwd — print working directory
#![no_std]
#![no_main]

use oxide_rt::{exit, getcwd, write};

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let mut buf = [0u8; 128];
    let n = getcwd(&mut buf);
    if n > 0 {
        write(1, &buf[..n as usize]);
        write(1, b"\n");
    } else {
        write(1, b"/\n");
    }
    exit(0);
}
