//! tail — output the last N lines (default 10)
//! Usage: tail [-n N] [file]
//! Reads stdin when no file is given.
#![no_std]
#![no_main]

use oxide_rt::{exit, read, write, open, close, arg, argc};

const STDIN:  i32 = 0;
const STDOUT: i32 = 1;

const BUF_SIZE: usize = 32768; // 32 KB input buffer
static mut BUF: [u8; BUF_SIZE] = [0u8; BUF_SIZE];

fn parse_n(s: &str) -> usize {
    let mut n = 0usize;
    for c in s.bytes() {
        if c.is_ascii_digit() { n = n * 10 + (c - b'0') as usize; } else { break; }
    }
    if n == 0 { 10 } else { n }
}

fn tail_fd(fd: i32, n_lines: usize) {
    // Read all input into BUF.
    let total = unsafe {
        let mut pos = 0usize;
        loop {
            if pos >= BUF_SIZE { break; }
            let chunk = read(fd, &mut BUF[pos..]);
            if chunk <= 0 { break; }
            pos += chunk as usize;
        }
        pos
    };

    let data = unsafe { &BUF[..total] };
    if total == 0 { return; }

    // Find the start of the last n_lines lines by scanning backwards.
    let mut newlines = 0usize;
    let mut start    = total;
    // Skip a trailing newline at the very end.
    let scan_end = if data.last() == Some(&b'\n') { total - 1 } else { total };
    let mut i = scan_end as isize - 1;
    while i >= 0 {
        if data[i as usize] == b'\n' {
            newlines += 1;
            if newlines >= n_lines {
                start = i as usize + 1;
                break;
            }
        }
        i -= 1;
    }
    if newlines < n_lines { start = 0; }

    let _ = write(STDOUT, &data[start..total]);
    // Ensure a trailing newline.
    if total > 0 && data[total - 1] != b'\n' {
        let _ = write(STDOUT, b"\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let mut n_lines = 10usize;
    let mut first_file = 1usize;

    if argc() > 1 {
        if let Some(a) = arg(1) {
            if a == "-n" {
                if let Some(v) = arg(2) { n_lines = parse_n(v); first_file = 3; }
            } else if a.starts_with("-n") {
                n_lines = parse_n(&a[2..]);
                first_file = 2;
            }
        }
    }

    let has_files = (first_file..argc()).any(|i| arg(i).is_some());
    if !has_files {
        tail_fd(STDIN, n_lines);
        exit(0);
    }

    for i in first_file..argc() {
        if let Some(path) = arg(i) {
            let fd = open(path, 0);
            if fd < 0 {
                let _ = write(STDOUT, b"tail: cannot open: ");
                let _ = write(STDOUT, path.as_bytes());
                let _ = write(STDOUT, b"\n");
                continue;
            }
            tail_fd(fd, n_lines);
            close(fd);
        }
    }
    exit(0);
}
