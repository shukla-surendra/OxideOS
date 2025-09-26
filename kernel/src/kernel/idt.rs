// src/kernel/idt.rs - 64-bit IDT implementation
#![no_std]

use core::mem::size_of;
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base: u64,
}

#[unsafe(no_mangle)]
static mut IDT_DESCRIPTOR: IdtDescriptor = IdtDescriptor { limit: 0, base: 0 };

// 64-bit IDT entry structure
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IdtEntry {
    offset_low: u16,     // bits 0-15 of handler address
    selector: u16,       // code segment selector
    ist: u8,            // interrupt stack table (bits 0-2), rest reserved
    flags: u8,          // type and attributes
    offset_mid: u16,    // bits 16-31 of handler address
    offset_high: u32,   // bits 32-63 of handler address
    reserved: u32,      // must be zero
}

impl IdtEntry {
    pub fn set_handler(&mut self, handler: unsafe extern "C" fn(), selector: u16, flags: u8) {
        let offset = handler as usize as u64;
        self.offset_low = (offset & 0xFFFF) as u16;
        self.selector = selector;
        self.ist = 0; // No IST for now
        self.flags = flags;
        self.offset_mid = ((offset >> 16) & 0xFFFF) as u16;
        self.offset_high = ((offset >> 32) & 0xFFFFFFFF) as u32;
        self.reserved = 0;
    }

    pub fn set_empty(&mut self) {
        *self = IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        };
    }
}

// The actual IDT - 256 entries for 64-bit
static mut IDT: [IdtEntry; 256] = [IdtEntry {
    offset_low: 0,
    selector: 0,
    ist: 0,
    flags: 0,
    offset_mid: 0,
    offset_high: 0,
    reserved: 0,
}; 256];

// External assembly interrupt handlers
unsafe extern "C" {
    // CPU exceptions
    unsafe fn isr0();   // Divide by zero
    unsafe fn isr1();   // Debug
    unsafe fn isr2();   // NMI
    unsafe fn isr3();   // Breakpoint
    unsafe fn isr4();   // Overflow
    unsafe fn isr5();   // Bound range exceeded
    unsafe fn isr6();   // Invalid opcode
    unsafe fn isr7();   // Device not available
    unsafe fn isr8();   // Double fault
    unsafe fn isr9();   // Coprocessor segment overrun
    unsafe fn isr10();  // Invalid TSS
    unsafe fn isr11();  // Segment not present
    unsafe fn isr12();  // Stack segment fault
    unsafe fn isr13();  // General protection fault
    unsafe fn isr14();  // Page fault
    unsafe fn isr15();  // Reserved
    unsafe fn isr16();  // x87 FPU error
    unsafe fn isr17();  // Alignment check
    unsafe fn isr18();  // Machine check
    unsafe fn isr19();  // SIMD floating-point
    unsafe fn isr20();  // Virtualization
    unsafe fn isr21();  // Control protection
    unsafe fn isr22();  // Reserved
    unsafe fn isr23();  // Reserved
    unsafe fn isr24();  // Reserved
    unsafe fn isr25();  // Reserved
    unsafe fn isr26();  // Reserved
    unsafe fn isr27();  // Reserved
    unsafe fn isr28();  // Hypervisor injection
    unsafe fn isr29();  // VMM communication
    unsafe fn isr30();  // Security
    unsafe fn isr31();  // Reserved

    // Hardware IRQs
    unsafe fn isr32();  // Timer (IRQ0)
    unsafe fn isr33();  // Keyboard (IRQ1)
    unsafe fn isr34();  // IRQ2
    unsafe fn isr35();  // IRQ3
    unsafe fn isr36();  // IRQ4
    unsafe fn isr37();  // IRQ5
    unsafe fn isr38();  // IRQ6
    unsafe fn isr39();  // IRQ7
    unsafe fn isr40();  // IRQ8
    unsafe fn isr41();  // IRQ9
    unsafe fn isr42();  // IRQ10
    unsafe fn isr43();  // IRQ11
    unsafe fn isr44();  // IRQ12
    unsafe fn isr45();  // IRQ13
    unsafe fn isr46();  // IRQ14
    unsafe fn isr47();  // IRQ15
}

pub fn init() {
    unsafe {
        // Get current code segment selector (should be different in 64-bit)
        let kernel_selector: u16;
        asm!("mov {0:x}, cs", out(reg) kernel_selector, options(nomem, nostack, preserves_flags));

        SERIAL_PORT.write_str("64-bit IDT: Using kernel selector: 0x");
        SERIAL_PORT.write_hex(kernel_selector as u32);
        SERIAL_PORT.write_str("\n");

        // Set up exception handlers (0-31)
        IDT[0].set_handler(isr0, kernel_selector, 0x8E);    // Interrupt gate
        IDT[1].set_handler(isr1, kernel_selector, 0x8E);
        IDT[2].set_handler(isr2, kernel_selector, 0x8E);
        IDT[3].set_handler(isr3, kernel_selector, 0x8E);
        IDT[4].set_handler(isr4, kernel_selector, 0x8E);
        IDT[5].set_handler(isr5, kernel_selector, 0x8E);
        IDT[6].set_handler(isr6, kernel_selector, 0x8E);
        IDT[7].set_handler(isr7, kernel_selector, 0x8E);
        IDT[8].set_handler(isr8, kernel_selector, 0x8E);    // Double fault - consider IST
        IDT[9].set_handler(isr9, kernel_selector, 0x8E);
        IDT[10].set_handler(isr10, kernel_selector, 0x8E);
        IDT[11].set_handler(isr11, kernel_selector, 0x8E);
        IDT[12].set_handler(isr12, kernel_selector, 0x8E);
        IDT[13].set_handler(isr13, kernel_selector, 0x8E);  // GPF
        IDT[14].set_handler(isr14, kernel_selector, 0x8E);  // Page fault
        IDT[15].set_handler(isr15, kernel_selector, 0x8E);
        IDT[16].set_handler(isr16, kernel_selector, 0x8E);
        IDT[17].set_handler(isr17, kernel_selector, 0x8E);
        IDT[18].set_handler(isr18, kernel_selector, 0x8E);  // Machine check - consider IST
        IDT[19].set_handler(isr19, kernel_selector, 0x8E);
        IDT[20].set_handler(isr20, kernel_selector, 0x8E);
        IDT[21].set_handler(isr21, kernel_selector, 0x8E);
        IDT[22].set_handler(isr22, kernel_selector, 0x8E);
        IDT[23].set_handler(isr23, kernel_selector, 0x8E);
        IDT[24].set_handler(isr24, kernel_selector, 0x8E);
        IDT[25].set_handler(isr25, kernel_selector, 0x8E);
        IDT[26].set_handler(isr26, kernel_selector, 0x8E);
        IDT[27].set_handler(isr27, kernel_selector, 0x8E);
        IDT[28].set_handler(isr28, kernel_selector, 0x8E);
        IDT[29].set_handler(isr29, kernel_selector, 0x8E);
        IDT[30].set_handler(isr30, kernel_selector, 0x8E);
        IDT[31].set_handler(isr31, kernel_selector, 0x8E);

        // Set up hardware IRQ handlers (32-47)
        IDT[32].set_handler(isr32, kernel_selector, 0x8E);  // Timer
        IDT[33].set_handler(isr33, kernel_selector, 0x8E);  // Keyboard
        IDT[34].set_handler(isr34, kernel_selector, 0x8E);
        IDT[35].set_handler(isr35, kernel_selector, 0x8E);
        IDT[36].set_handler(isr36, kernel_selector, 0x8E);
        IDT[37].set_handler(isr37, kernel_selector, 0x8E);
        IDT[38].set_handler(isr38, kernel_selector, 0x8E);
        IDT[39].set_handler(isr39, kernel_selector, 0x8E);
        IDT[40].set_handler(isr40, kernel_selector, 0x8E);
        IDT[41].set_handler(isr41, kernel_selector, 0x8E);
        IDT[42].set_handler(isr42, kernel_selector, 0x8E);
        IDT[43].set_handler(isr43, kernel_selector, 0x8E);
        IDT[44].set_handler(isr44, kernel_selector, 0x8E);
        IDT[45].set_handler(isr45, kernel_selector, 0x8E);
        IDT[46].set_handler(isr46, kernel_selector, 0x8E);
        IDT[47].set_handler(isr47, kernel_selector, 0x8E);

        // Default handler for unused entries
        unsafe extern "C" fn default_isr() {
            SERIAL_PORT.write_str("[DEFAULT_64BIT_ISR]\n");
        }

        // Set default handlers for remaining entries
        for i in 48..256 {
            IDT[i].set_handler(default_isr, kernel_selector, 0x8E);
        }

        // Set up IDT descriptor
        let idt_limit = (size_of::<[IdtEntry; 256]>() - 1) as u16;
        let idt_base = core::ptr::addr_of_mut!(IDT) as *const _ as usize as u64;

        IDT_DESCRIPTOR.limit = idt_limit;
        IDT_DESCRIPTOR.base = idt_base;

        SERIAL_PORT.write_str("64-bit IDT base: 0x");
        SERIAL_PORT.write_hex((idt_base >> 32) as u32);
        SERIAL_PORT.write_hex(idt_base as u32);
        SERIAL_PORT.write_str(", limit: 0x");
        SERIAL_PORT.write_hex(idt_limit as u32);
        SERIAL_PORT.write_str("\n");

        // Load IDT
        core::arch::asm!("lidt [{}]", sym IDT_DESCRIPTOR, options(nostack, preserves_flags));

        // Verify IDT was loaded
        let mut readback: [u8; 10] = [0u8; 10]; // 64-bit needs 10 bytes (2 + 8)
        core::arch::asm!("sidt [{}]", in(reg) &mut readback, options(nostack, preserves_flags));
        
        let rb_limit = u16::from_le_bytes([readback[0], readback[1]]);
        let rb_base = u64::from_le_bytes([
            readback[2], readback[3], readback[4], readback[5],
            readback[6], readback[7], readback[8], readback[9]
        ]);

        SERIAL_PORT.write_str("IDT readback - base: 0x");
        SERIAL_PORT.write_hex((rb_base >> 32) as u32);
        SERIAL_PORT.write_hex(rb_base as u32);
        SERIAL_PORT.write_str(", limit: 0x");
        SERIAL_PORT.write_hex(rb_limit as u32);
        SERIAL_PORT.write_str("\n");

        SERIAL_PORT.write_str("64-bit IDT loaded successfully\n");
    }
}