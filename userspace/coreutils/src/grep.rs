//! grep — filter lines matching a pattern
//! Usage: grep <pattern> [file ...]
//! Reads stdin when no file is given. Performs substring matching.
#![no_std]
#![no_main]

use oxide_rt::{exit, read, write, open, close, arg, argc};

const STDIN:  i32 = 0;
const STDOUT: i32 = 1;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > haystack.len() { return false; }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle { return true; }
    }
    false
}

fn grep_fd(fd: i32, pattern: &[u8]) -> bool {
    let mut line   = [0u8; 1024];
    let mut llen   = 0usize;
    let mut input  = [0u8; 512];
    let mut matched = false;
    loop {
        let n = read(fd, &mut input);
        if n <= 0 { break; }
        for &b in &input[..n as usize] {
            if b == b'\n' {
                if contains(&line[..llen], pattern) {
                    let _ = write(STDOUT, &line[..llen]);
                    let _ = write(STDOUT, b"\n");
                    matched = true;
                }
                llen = 0;
            } else if llen < line.len() - 1 {
                line[llen] = b;
                llen += 1;
            }
        }
    }
    // Flush any remaining partial line (no trailing newline in input).
    if llen > 0 && contains(&line[..llen], pattern) {
        let _ = write(STDOUT, &line[..llen]);
        let _ = write(STDOUT, b"\n");
        matched = true;
    }
    matched
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    if argc() < 2 {
        let _ = write(STDOUT, b"Usage: grep <pattern> [file ...]\n");
        exit(1);
    }
    let pattern = match arg(1) {
        Some(p) => p.as_bytes(),
        None    => exit(1),
    };

    let mut any_match = false;
    if argc() == 2 {
        if grep_fd(STDIN, pattern) { any_match = true; }
    } else {
        for i in 2..argc() {
            if let Some(path) = arg(i) {
                let fd = open(path, 0);
                if fd < 0 {
                    let _ = write(STDOUT, b"grep: cannot open: ");
                    let _ = write(STDOUT, path.as_bytes());
                    let _ = write(STDOUT, b"\n");
                    continue;
                }
                if grep_fd(fd, pattern) { any_match = true; }
                close(fd);
            }
        }
    }
    exit(if any_match { 0 } else { 1 });
}
