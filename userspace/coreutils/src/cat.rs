//! cat — concatenate and print files
//! Usage: cat <file> [file2 ...]
#![no_std]
#![no_main]

use oxide_rt::{exit, open, close, read, write, arg, argc};

const O_RDONLY: u32 = 0;
const STDOUT: i32 = 1;

fn cat_fd(fd: i32) {
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        write(STDOUT, &buf[..n as usize]);
    }
}

fn print_bytes_inline(b: &[u8]) { write(STDOUT, b); }

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    if argc() < 2 {
        print_bytes_inline(b"Usage: cat <file> [file2 ...]\n");
        exit(1);
    }

    let mut any_error = false;
    for i in 1..argc() {
        let path = match arg(i) {
            Some(p) => p,
            None    => continue,
        };
        let fd = open(path, O_RDONLY);
        if fd < 0 {
            print_bytes_inline(b"cat: cannot open: ");
            print_bytes_inline(path.as_bytes());
            print_bytes_inline(b"\n");
            any_error = true;
            continue;
        }
        cat_fd(fd);
        close(fd);
    }

    exit(if any_error { 1 } else { 0 });
}
