//! true — exit with success (0)
#![no_std]
#![no_main]
use oxide_rt::exit;
#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() { exit(0); }
