//! sleep — pause for N seconds
//! Usage: sleep <seconds>
#![no_std]
#![no_main]

use oxide_rt::{exit, sleep_ms, write, arg, argc};

const STDOUT: i32 = 1;

fn parse_secs(s: &str) -> u64 {
    let mut n = 0u64;
    for c in s.bytes() {
        if c.is_ascii_digit() { n = n * 10 + (c - b'0') as u64; } else { break; }
    }
    n
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    if argc() < 2 {
        let _ = write(STDOUT, b"Usage: sleep <seconds>\n");
        exit(1);
    }
    let secs = match arg(1) { Some(s) => parse_secs(s), None => exit(1) };
    sleep_ms(secs * 1000);
    exit(0);
}
