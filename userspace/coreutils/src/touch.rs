//! touch — create files if they do not exist
//! Usage: touch <file> [file ...]
#![no_std]
#![no_main]

use oxide_rt::{exit, open, close, write, arg, argc};

const STDOUT:  i32 = 1;
const O_WRONLY: u32 = 1;
const O_CREAT:  u32 = 0x40;

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    if argc() < 2 {
        let _ = write(STDOUT, b"Usage: touch <file> [file ...]\n");
        exit(1);
    }
    let mut any_error = false;
    for i in 1..argc() {
        if let Some(path) = arg(i) {
            let fd = open(path, O_WRONLY | O_CREAT);
            if fd < 0 {
                let _ = write(STDOUT, b"touch: cannot create: ");
                let _ = write(STDOUT, path.as_bytes());
                let _ = write(STDOUT, b"\n");
                any_error = true;
            } else {
                close(fd);
            }
        }
    }
    exit(if any_error { 1 } else { 0 });
}
