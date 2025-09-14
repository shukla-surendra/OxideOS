// src/pic.rs
#![no_std]

use super::ports::{inb, outb};

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;

/// Remap the PIC so hardware IRQs start at `offset_master` (commonly 0x20)
/// Why: By default PIC IRQs overlap CPU exceptions (vectors 0..15). 
/// Remap them to 32..47 (0x20..0x2F) so they don’t collide. 
/// Always send EOI after handling to allow future interrupts.
pub unsafe fn remap(offset_master: u8, offset_slave: u8) {
    let a1 = inb(PIC1_DATA);
    let a2 = inb(PIC2_DATA);

    outb(PIC1_CMD, ICW1_INIT);
    outb(PIC2_CMD, ICW1_INIT);
    outb(PIC1_DATA, offset_master);
    outb(PIC2_DATA, offset_slave);
    outb(PIC1_DATA, 4); // tell master there is a slave on IRQ2
    outb(PIC2_DATA, 2); // tell slave its identity
    outb(PIC1_DATA, ICW4_8086);
    outb(PIC2_DATA, ICW4_8086);

    outb(PIC1_DATA, a1); // restore saved masks
    outb(PIC2_DATA, a2);
}

/// Send End Of Interrupt for IRQ `irq` (0..15)
pub unsafe fn send_eoi(irq: u8) {
    if irq >= 8 {
        outb(PIC2_CMD, 0x20);
    }
    outb(PIC1_CMD, 0x20);
}
