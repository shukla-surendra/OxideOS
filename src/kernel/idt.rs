// src/kernel/idt.rs
#![no_std]

use core::mem::size_of;
use core::arch::asm;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    zero: u8,
    attributes: u8,
    offset_high: u16,
}

impl IdtEntry {
    pub const fn new() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            zero: 0,
            attributes: 0,
            offset_high: 0,
        }
    }

    /// Note: handler_type is `unsafe extern "C" fn()` to match ISR symbol types declared
    /// inside `unsafe extern "C" { fn isr32(); }` blocks.
    pub fn set_handler(&mut self, handler: unsafe extern "C" fn(), selector: u16, attributes: u8) {
        let addr = handler as usize;
        self.offset_low = (addr & 0xFFFF) as u16;
        self.selector = selector;
        self.zero = 0;
        self.attributes = attributes;
        self.offset_high = ((addr >> 16) & 0xFFFF) as u16;
    }
}

#[repr(C, packed)]
pub struct IdtPointer {
    limit: u16,
    base: u32,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::new(); 256];

/// Public API: accept `unsafe extern "C" fn()` so callers can pass symbols declared
/// in `unsafe extern "C"` blocks without casting.
pub unsafe fn set_idt_entry(idx: usize, handler: unsafe extern "C" fn(), selector: u16, attributes: u8) {
    IDT[idx].set_handler(handler, selector, attributes);
}

pub unsafe fn load_idt() {
    let idt_ptr = IdtPointer {
        limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: &raw const IDT as *const _ as u32,
    };
    asm!(
        "lidt [{0}]",
        in(reg) &idt_ptr,
        options(nostack, preserves_flags)
    );
}
