// src/kernel/ports.rs
/*
An IRQ (Interrupt Request) is a hardware signal sent to a computer's CPU 
to temporarily halt its current operations and allow another device to get the processor's 
attention for processing or data transfer.

we need these to talk to PIC and keyboard ports
*/
#![no_std]

use core::arch::asm;

#[inline]
pub unsafe fn outb(port: u16, val: u8) {
    // PIC and keyboard require port I/O. outb/inb are the standard x86 instructions to write/read I/O ports.
    asm!("out dx, al", in("dx") port, in("al") val, options(nostack, nomem));
}

#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    // PIC and keyboard require port I/O. outb/inb are the standard x86 instructions to write/read I/O ports.
    let mut v: u8;
    asm!("in al, dx", in("dx") port, out("al") v, options(nostack, nomem));
    v
}
