// src/kernel/idt.rs
#![no_std]

use core::mem::size_of;
use core::ptr::addr_of;
use core::arch::asm;


use crate::kernel::serial::SERIAL_PORT;

#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base: u32,
}

#[unsafe(no_mangle)]
static mut IDT_DESCRIPTOR: IdtDescriptor = IdtDescriptor { limit: 0, base: 0 };

// keep IDT in .bss/data as a static so its address is stable
#[repr(C, packed)]
#[derive(Copy)]
#[derive(Clone)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    zero: u8,
    flags: u8,
    offset_high: u16,
}

impl IdtEntry {
    pub fn set_handler(&mut self, handler: unsafe extern "C" fn(), selector: u16, flags: u8) {
        let offset = handler as usize as u32;
        self.offset_low = (offset & 0xFFFF) as u16;
        self.selector = selector;
        self.zero = 0;
        self.flags = flags;
        self.offset_high = ((offset >> 16) & 0xFFFF) as u16;
    }
}

// The actual IDT
static mut IDT: [IdtEntry; 256] = [IdtEntry {
    offset_low: 0,
    selector: 0,
    zero: 0,
    flags: 0,
    offset_high: 0,
}; 256];

// External assembly interrupt handlers - must be marked unsafe
unsafe extern "C" {
    // CPU exceptions
    unsafe fn isr0();
    unsafe fn isr1();
    unsafe fn isr2();
    unsafe fn isr3();
    unsafe fn isr4();
    unsafe fn isr5();
    unsafe fn isr6();
    unsafe fn isr7();
    unsafe fn isr8();
    unsafe fn isr9();
    unsafe fn isr10();
    unsafe fn isr11();
    unsafe fn isr12();
    unsafe fn isr13();
    unsafe fn isr14();
    unsafe fn isr15();
    unsafe fn isr16();
    unsafe fn isr17();
    unsafe fn isr18();
    unsafe fn isr19();
    unsafe fn isr20();
    unsafe fn isr21();
    unsafe fn isr22();
    unsafe fn isr23();
    unsafe fn isr24();
    unsafe fn isr25();
    unsafe fn isr26();
    unsafe fn isr27();
    unsafe fn isr28();
    unsafe fn isr29();
    unsafe fn isr30();
    unsafe fn isr31();

    // IRQs
    unsafe fn isr32();
    unsafe fn isr33();
    unsafe fn isr34();
    unsafe fn isr35();
    unsafe fn isr36();
    unsafe fn isr37();
    unsafe fn isr38();
    unsafe fn isr39();
    unsafe fn isr40();
    unsafe fn isr41();
    unsafe fn isr42();
    unsafe fn isr43();
    unsafe fn isr44();
    unsafe fn isr45();
    unsafe fn isr46();
    unsafe fn isr47();
}

pub fn init() {
    unsafe {
        // Exceptions: set handlers for 0..31
        IDT[0].set_handler(isr0, 0x08, 0x8E);
        IDT[1].set_handler(isr1, 0x08, 0x8E);
        IDT[2].set_handler(isr2, 0x08, 0x8E);
        IDT[3].set_handler(isr3, 0x08, 0x8E);
        IDT[4].set_handler(isr4, 0x08, 0x8E);
        IDT[5].set_handler(isr5, 0x08, 0x8E);
        IDT[6].set_handler(isr6, 0x08, 0x8E);
        IDT[7].set_handler(isr7, 0x08, 0x8E);
        IDT[8].set_handler(isr8, 0x08, 0x8E);   // double fault etc
        IDT[9].set_handler(isr9, 0x08, 0x8E);
        IDT[10].set_handler(isr10, 0x08, 0x8E);
        IDT[11].set_handler(isr11, 0x08, 0x8E);
        IDT[12].set_handler(isr12, 0x08, 0x8E);
        IDT[13].set_handler(isr13, 0x08, 0x8E);
        IDT[14].set_handler(isr14, 0x08, 0x8E);
        IDT[15].set_handler(isr15, 0x08, 0x8E);
        IDT[16].set_handler(isr16, 0x08, 0x8E);
        IDT[17].set_handler(isr17, 0x08, 0x8E);
        IDT[18].set_handler(isr18, 0x08, 0x8E);
        IDT[19].set_handler(isr19, 0x08, 0x8E);
        IDT[20].set_handler(isr20, 0x08, 0x8E);
        IDT[21].set_handler(isr21, 0x08, 0x8E);
        IDT[22].set_handler(isr22, 0x08, 0x8E);
        IDT[23].set_handler(isr23, 0x08, 0x8E);
        IDT[24].set_handler(isr24, 0x08, 0x8E);
        IDT[25].set_handler(isr25, 0x08, 0x8E);
        IDT[26].set_handler(isr26, 0x08, 0x8E);
        IDT[27].set_handler(isr27, 0x08, 0x8E);
        IDT[28].set_handler(isr28, 0x08, 0x8E);
        IDT[29].set_handler(isr29, 0x08, 0x8E);
        IDT[30].set_handler(isr30, 0x08, 0x8E);
        IDT[31].set_handler(isr31, 0x08, 0x8E);

        // IRQs (32..47)
        IDT[32].set_handler(isr32, 0x08, 0x8E);
        IDT[33].set_handler(isr33, 0x08, 0x8E);
        IDT[34].set_handler(isr34, 0x08, 0x8E);
        IDT[35].set_handler(isr35, 0x08, 0x8E);
        IDT[36].set_handler(isr36, 0x08, 0x8E);
        IDT[37].set_handler(isr37, 0x08, 0x8E);
        IDT[38].set_handler(isr38, 0x08, 0x8E);
        IDT[39].set_handler(isr39, 0x08, 0x8E);
        IDT[40].set_handler(isr40, 0x08, 0x8E);
        IDT[41].set_handler(isr41, 0x08, 0x8E);
        IDT[42].set_handler(isr42, 0x08, 0x8E);
        IDT[43].set_handler(isr43, 0x08, 0x8E);
        IDT[44].set_handler(isr44, 0x08, 0x8E);
        IDT[45].set_handler(isr45, 0x08, 0x8E);
        IDT[46].set_handler(isr46, 0x08, 0x8E);
        IDT[47].set_handler(isr47, 0x08, 0x8E);

        // Fill the rest with a default handler if desired
        // for i in 48..256 { IDT[i].set_handler(default_isr, 0x08, 0x8E); }

        // Build static descriptor
        let idt_limit = (size_of::<[IdtEntry; 256]>() - 1) as u16;
        let idt_base = core::ptr::addr_of_mut!(IDT) as *const _ as usize as u32;

        IDT_DESCRIPTOR.limit = idt_limit;
        IDT_DESCRIPTOR.base = idt_base;

        SERIAL_PORT.write_str("  (dbg) IDT.as_ptr(): 0x");
        SERIAL_PORT.write_hex(idt_base);
        SERIAL_PORT.write_str(", descriptor at: 0x");
        SERIAL_PORT.write_hex(core::ptr::addr_of_mut!(IDT_DESCRIPTOR) as *const () as usize as u32);
        SERIAL_PORT.write_str(", limit: 0x");
        SERIAL_PORT.write_hex(idt_limit as u32);
        SERIAL_PORT.write_str("\n");

        // Load IDT via symbol address (stable)
        core::arch::asm!("lidt [{}]", sym IDT_DESCRIPTOR, options(nostack, preserves_flags));

        // Readback (sidt) to validate it actually loaded
        let mut readback: [u8; 6] = [0u8; 6];
        core::arch::asm!("sidt [{}]", in(reg) &mut readback, options(nostack, preserves_flags));
        let rb_limit = u16::from_le_bytes([readback[0], readback[1]]);
        let rb_base = u32::from_le_bytes([readback[2], readback[3], readback[4], readback[5]]);

        SERIAL_PORT.write_str("  (dbg) IDT readback after lidt - base: 0x");
        SERIAL_PORT.write_hex(rb_base);
        SERIAL_PORT.write_str(", limit: 0x");
        SERIAL_PORT.write_hex(rb_limit as u32);
        SERIAL_PORT.write_str("\n");

        SERIAL_PORT.write_str("  ✓ IDT loaded\n");
    }
}