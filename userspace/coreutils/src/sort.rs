//! sort — sort lines of text
//! Usage: sort [file]
//! Reads stdin when no file is given. Sorts lexicographically.
#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;
use oxide_rt::{exit, read, write, open, close, arg, argc};

const STDIN:  i32 = 0;
const STDOUT: i32 = 1;

fn read_all(fd: i32) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        data.extend_from_slice(&buf[..n as usize]);
    }
    data
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let first_file = 1usize;

    let has_files = (first_file..argc()).any(|i| arg(i).is_some());
    let data = if !has_files {
        read_all(STDIN)
    } else {
        let mut all = Vec::new();
        for i in first_file..argc() {
            if let Some(path) = arg(i) {
                let fd = open(path, 0);
                if fd < 0 {
                    let _ = write(STDOUT, b"sort: cannot open: ");
                    let _ = write(STDOUT, path.as_bytes());
                    let _ = write(STDOUT, b"\n");
                    continue;
                }
                all.extend_from_slice(&read_all(fd));
                close(fd);
            }
        }
        all
    };

    // Split into lines.
    let mut lines: Vec<&[u8]> = data.split(|&b| b == b'\n').filter(|l| !l.is_empty()).collect();

    // Sort: simple insertion sort (stable, correct for small inputs).
    let n = lines.len();
    for i in 1..n {
        let mut j = i;
        while j > 0 && lines[j - 1] > lines[j] {
            lines.swap(j - 1, j);
            j -= 1;
        }
    }

    for line in &lines {
        let _ = write(STDOUT, line);
        let _ = write(STDOUT, b"\n");
    }
    exit(0);
}
