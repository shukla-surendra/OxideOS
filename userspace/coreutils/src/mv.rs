//! mv — rename/move a file.
//! Prompts for source and destination (until argv is wired up).
#![no_std]
#![no_main]

use oxide_rt::{exit, write, rename, getchar, sleep_ms};

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
    write(1, b"mv: source: ");
    let mut src = [0u8; 256];
    let sn = readline(&mut src);
    if sn == 0 { write(1, b"mv: no source\n"); exit(1); }

    write(1, b"mv: destination: ");
    let mut dst = [0u8; 256];
    let dn = readline(&mut dst);
    if dn == 0 { write(1, b"mv: no destination\n"); exit(1); }

    let s = unsafe { core::str::from_utf8_unchecked(&src[..sn]) };
    let d = unsafe { core::str::from_utf8_unchecked(&dst[..dn]) };

    let r = rename(s, d);
    if r < 0 {
        write(1, b"mv: cannot rename '");
        write(1, s.as_bytes());
        write(1, b"' to '");
        write(1, d.as_bytes());
        write(1, b"'\n");
        exit(1);
    }
    write(1, b"mv: '");
    write(1, s.as_bytes());
    write(1, b"' -> '");
    write(1, d.as_bytes());
    write(1, b"'\n");
    exit(0);
}
