//! ls — list directory contents
#![no_std]
#![no_main]

use oxide_rt::{exit, getcwd, readdir, print_str, print_bytes};

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    // TODO: parse argv once we have it; for now read argv from env or use cwd
    // Use getcwd as the default path
    let mut cwd_buf = [0u8; 128];
    let cwd_len = getcwd(&mut cwd_buf);
    let path = if cwd_len > 0 {
        unsafe { core::str::from_utf8_unchecked(&cwd_buf[..cwd_len as usize]) }
    } else {
        "/"
    };

    let mut buf = [0u8; 4096];
    let n = readdir(path, &mut buf);
    if n < 0 {
        print_str("ls: cannot read directory\n");
        exit(1);
    }

    // Each entry is "<name>\n"; print them with some formatting
    let entries = unsafe { core::str::from_utf8_unchecked(&buf[..n as usize]) };
    let mut count = 0usize;
    for entry in entries.split('\n') {
        if entry.is_empty() { continue; }
        print_str("  ");
        print_bytes(entry.as_bytes());
        print_str("\n");
        count += 1;
    }

    if count == 0 {
        print_str("(empty)\n");
    }

    exit(0);
}
