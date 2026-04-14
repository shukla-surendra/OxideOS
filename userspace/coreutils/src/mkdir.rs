//! mkdir — create a directory
#![no_std]
#![no_main]

use oxide_rt::{exit, mkdir, write, getchar, sleep_ms};

fn getchar_block() -> u8 {
    loop { if let Some(c) = getchar() { return c; } sleep_ms(5); }
}

fn readline(buf: &mut [u8]) -> usize {
    let mut len = 0;
    loop {
        let c = getchar_block();
        if len < buf.len() { buf[len] = c; len += 1; }
        if c == b'\n' { break; }
    }
    if len > 0 && buf[len - 1] == b'\n' { len - 1 } else { len }
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    write(1, b"mkdir: path: ");
    let mut buf = [0u8; 128];
    let len = readline(&mut buf);
    if len == 0 { write(1, b"usage: mkdir <path>\n"); exit(1); }
    let path = unsafe { core::str::from_utf8_unchecked(&buf[..len]) };
    let r = mkdir(path);
    if r == 0 {
        write(1, b"directory created\n");
        exit(0);
    } else {
        write(1, b"mkdir: failed\n");
        exit(1);
    }
}
