//! cat — concatenate and print files
//! Usage: cat <file> [file2 ...]
//! Reads file paths from argv (stored at fixed address by _start when available),
//! or reads from stdin (fd=0) if no args.
#![no_std]
#![no_main]

use oxide_rt::{exit, open, close, read, write, getchar, sleep_ms};

const O_RDONLY: u32 = 0;
const STDOUT: i32 = 1;

/// Blocking getchar
fn getchar_block() -> u8 {
    loop {
        if let Some(c) = getchar() { return c; }
        sleep_ms(5);
    }
}

/// Read a line from stdin into buf; returns length (including '\n').
fn readline_stdin(buf: &mut [u8]) -> usize {
    let mut len = 0;
    loop {
        let c = getchar_block();
        if len < buf.len() {
            buf[len] = c;
            len += 1;
        }
        if c == b'\n' { break; }
    }
    len
}

fn cat_fd(fd: i32) {
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        write(STDOUT, &buf[..n as usize]);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    // No argv support yet — read a filename from stdin as a simple workaround.
    // When argv is added to oxide-rt, this will be replaced.
    print_inline(b"cat: enter filename: ");
    let mut path_buf = [0u8; 128];
    let len = readline_stdin(&mut path_buf);
    // strip trailing newline
    let end = if len > 0 && path_buf[len - 1] == b'\n' { len - 1 } else { len };
    if end == 0 {
        // No filename — cat stdin to stdout
        cat_fd(0);
        exit(0);
    }

    let path = unsafe { core::str::from_utf8_unchecked(&path_buf[..end]) };
    let fd = open(path, O_RDONLY);
    if fd < 0 {
        print_inline(b"cat: cannot open file\n");
        exit(1);
    }
    cat_fd(fd);
    close(fd);
    exit(0);
}

fn print_inline(b: &[u8]) {
    write(STDOUT, b);
}
