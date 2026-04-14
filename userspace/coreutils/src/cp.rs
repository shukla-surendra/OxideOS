//! cp — copy a file
//! Prompts for src and dst paths (until argv is wired up).
#![no_std]
#![no_main]

use oxide_rt::{exit, open, close, read, write, getchar, sleep_ms};

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_CREAT:  u32 = 0x40;
const O_TRUNC:  u32 = 0x200;
const STDOUT: i32 = 1;

fn getchar_block() -> u8 {
    loop {
        if let Some(c) = getchar() { return c; }
        sleep_ms(5);
    }
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
    write(STDOUT, b"cp: source: ");
    let mut src_buf = [0u8; 128];
    let src_len = readline(&mut src_buf);

    write(STDOUT, b"cp: destination: ");
    let mut dst_buf = [0u8; 128];
    let dst_len = readline(&mut dst_buf);

    if src_len == 0 || dst_len == 0 {
        write(STDOUT, b"cp: missing path\n");
        exit(1);
    }

    let src = unsafe { core::str::from_utf8_unchecked(&src_buf[..src_len]) };
    let dst = unsafe { core::str::from_utf8_unchecked(&dst_buf[..dst_len]) };

    let src_fd = open(src, O_RDONLY);
    if src_fd < 0 {
        write(STDOUT, b"cp: cannot open source\n");
        exit(1);
    }

    let dst_fd = open(dst, O_WRONLY | O_CREAT | O_TRUNC);
    if dst_fd < 0 {
        close(src_fd);
        write(STDOUT, b"cp: cannot open destination\n");
        exit(1);
    }

    let mut buf = [0u8; 512];
    let mut total = 0u64;
    loop {
        let n = read(src_fd, &mut buf);
        if n <= 0 { break; }
        write(dst_fd, &buf[..n as usize]);
        total += n as u64;
    }

    close(src_fd);
    close(dst_fd);

    write(STDOUT, b"cp: copied ");
    print_u64(total);
    write(STDOUT, b" bytes\n");
    exit(0);
}

fn print_u64(mut n: u64) {
    if n == 0 { write(1, b"0"); return; }
    let mut buf = [0u8; 20];
    let mut i = 20usize;
    while n > 0 { i -= 1; buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    write(1, &buf[i..]);
}
