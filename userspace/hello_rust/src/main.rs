#![no_std]
#![no_main]

use oxide_rt::{println, print, sleep_ms, get_time};

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    println!("Hello from Rust on OxideOS!");

    let t = get_time();
    println!("Current tick: {}", t);

    print!("Counting: ");
    for i in 1..=5 {
        print!("{} ", i);
        sleep_ms(200);
    }
    println!();

    println!("Done!");
}
