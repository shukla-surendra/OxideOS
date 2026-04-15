//! rm — remove files.
//! Prompts for filename (until argv is wired up).
#![no_std]
#![no_main]

use oxide_rt::{exit, write, unlink, getchar, sleep_ms};

fn getchar_block() -> u8 {
    loop { if let Some(c) = getchar() { return c; } sleep_ms(5); }
}

fn readline(buf: &mut [u8]) -> usize {
    let mut len = 0;
    loop {
        let c = getchar_block();
        if c == b'\n' || c == b'\r' { break; }
        if (c == 8 || c == 127) && len > 0 { len -= 1; } else if len < buf.len() { buf[len] = c; len += 1; }
    }
    len
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    write(1, b"rm: file to remove: ");
    let mut buf = [0u8; 256];
    let n = readline(&mut buf);
    if n == 0 { write(1, b"rm: no filename\n"); exit(1); }

    let path = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };
    let r = unlink(path);
    if r < 0 {
        write(1, b"rm: cannot remove '");
        write(1, path.as_bytes());
        write(1, b"'\n");
        exit(1);
    }
    write(1, b"rm: removed '");
    write(1, path.as_bytes());
    write(1, b"'\n");
    exit(0);
}
