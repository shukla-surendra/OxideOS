//! echo — print arguments to stdout
//! Usage: echo [arg ...]
#![no_std]
#![no_main]

use oxide_rt::{exit, write, arg, argc};

const STDOUT: i32 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let n = argc();
    let mut first = true;
    for i in 1..n {
        if !first { let _ = write(STDOUT, b" "); }
        if let Some(a) = arg(i) { let _ = write(STDOUT, a.as_bytes()); }
        first = false;
    }
    let _ = write(STDOUT, b"\n");
    exit(0);
}
