//! wc — word, line, and byte count
//! Usage: wc [-l|-w|-c] [file ...]
//! Flags: -l lines only, -w words only, -c bytes only (default: all three)
#![no_std]
#![no_main]

use oxide_rt::{exit, read, write, open, close, arg, argc};

const STDIN:  i32 = 0;
const STDOUT: i32 = 1;

struct Counts { bytes: u64, words: u64, lines: u64 }

fn count_fd(fd: i32) -> Counts {
    let mut c = Counts { bytes: 0, words: 0, lines: 0 };
    let mut buf = [0u8; 512];
    let mut in_word = false;
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        for &b in &buf[..n as usize] {
            c.bytes += 1;
            if b == b'\n' { c.lines += 1; }
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                in_word = false;
            } else if !in_word {
                in_word = true;
                c.words += 1;
            }
        }
    }
    c
}

fn write_u64(n: u64) {
    if n == 0 { let _ = write(STDOUT, b"0"); return; }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut v = n;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let _ = write(STDOUT, &buf[i..]);
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    // Parse flags.
    let (flag_l, flag_w, flag_c) = {
        let mut l = false; let mut w = false; let mut c = false;
        let mut has_flag = false;
        for i in 1..argc() {
            if let Some(a) = arg(i) {
                if a.starts_with('-') {
                    has_flag = true;
                    for ch in a.bytes().skip(1) {
                        match ch { b'l' => l = true, b'w' => w = true, b'c' => c = true, _ => {} }
                    }
                }
            }
        }
        if !has_flag { l = true; w = true; c = true; }
        (l, w, c)
    };

    // Collect file arguments (skip flags).
    let mut file_count = 0usize;
    let mut total = Counts { bytes: 0, words: 0, lines: 0 };

    // Check if there are any file args.
    let has_files = (1..argc()).any(|i| arg(i).map(|a| !a.starts_with('-')).unwrap_or(false));

    let print_counts = |c: &Counts| {
        let mut first = true;
        if flag_l { if !first { let _ = write(STDOUT, b" "); } write_u64(c.lines); first = false; }
        if flag_w { if !first { let _ = write(STDOUT, b" "); } write_u64(c.words); first = false; }
        if flag_c { if !first { let _ = write(STDOUT, b" "); } write_u64(c.bytes); first = false; }
    };

    if !has_files {
        let c = count_fd(STDIN);
        print_counts(&c);
        let _ = write(STDOUT, b"\n");
        exit(0);
    }

    for i in 1..argc() {
        if let Some(path) = arg(i) {
            if path.starts_with('-') { continue; }
            let fd = open(path, 0);
            if fd < 0 {
                let _ = write(STDOUT, b"wc: cannot open: ");
                let _ = write(STDOUT, path.as_bytes());
                let _ = write(STDOUT, b"\n");
                continue;
            }
            let c = count_fd(fd);
            close(fd);
            total.bytes += c.bytes;
            total.words += c.words;
            total.lines += c.lines;
            print_counts(&c);
            let _ = write(STDOUT, b" ");
            let _ = write(STDOUT, path.as_bytes());
            let _ = write(STDOUT, b"\n");
            file_count += 1;
        }
    }

    if file_count > 1 {
        print_counts(&total);
        let _ = write(STDOUT, b" total\n");
    }
    exit(0);
}
