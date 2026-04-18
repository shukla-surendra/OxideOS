//! head — output the first N lines (default 10)
//! Usage: head [-n N] [file ...]
#![no_std]
#![no_main]

use oxide_rt::{exit, read, write, open, close, arg, argc};

const STDIN:  i32 = 0;
const STDOUT: i32 = 1;

fn parse_n(s: &str) -> usize {
    let mut n = 0usize;
    for c in s.bytes() {
        if c.is_ascii_digit() { n = n * 10 + (c - b'0') as usize; } else { break; }
    }
    if n == 0 { 10 } else { n }
}

fn head_fd(fd: i32, max_lines: usize) {
    let mut buf   = [0u8; 512];
    let mut lines = 0usize;
    let mut i     = 0usize;
    let mut n_buf = 0usize;
    loop {
        if i >= n_buf {
            let n = read(fd, &mut buf);
            if n <= 0 { break; }
            n_buf = n as usize;
            i = 0;
        }
        let b = buf[i];
        i += 1;
        let _ = write(STDOUT, core::slice::from_ref(&b));
        if b == b'\n' {
            lines += 1;
            if lines >= max_lines { break; }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let mut max_lines = 10usize;
    let mut first_file = 1usize;

    // Parse -n N flag.
    if argc() > 1 {
        if let Some(a) = arg(1) {
            if a == "-n" || a.starts_with("-n") {
                if a == "-n" {
                    if let Some(v) = arg(2) { max_lines = parse_n(v); first_file = 3; }
                } else {
                    max_lines = parse_n(&a[2..]);
                    first_file = 2;
                }
            }
        }
    }

    let has_files = (first_file..argc()).any(|i| arg(i).map(|_| true).unwrap_or(false));
    if !has_files {
        head_fd(STDIN, max_lines);
        exit(0);
    }

    for i in first_file..argc() {
        if let Some(path) = arg(i) {
            let fd = open(path, 0);
            if fd < 0 {
                let _ = write(STDOUT, b"head: cannot open: ");
                let _ = write(STDOUT, path.as_bytes());
                let _ = write(STDOUT, b"\n");
                continue;
            }
            head_fd(fd, max_lines);
            close(fd);
        }
    }
    exit(0);
}
