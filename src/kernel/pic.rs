// src/kernel/pic.rs
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const PIC_EOI: u8 = 0x20;

/// Send End-of-Interrupt to PIC
pub unsafe fn send_eoi(irq: u8) {
    if irq >= 8 {
        asm!("out dx, al", in("dx") PIC2_COMMAND, in("al") PIC_EOI);
        unsafe { io_wait() };
    }
    asm!("out dx, al", in("dx") PIC1_COMMAND, in("al") PIC_EOI);
    unsafe { io_wait() };
}

/// Small delay for PIC initialization
#[inline]
pub unsafe fn io_wait() {
    asm!("out 0x80, al", in("al") 0u8); // Write to unused port for delay
}

/// Initialize the PIC
pub unsafe fn init() {
    // Save masks
    let mask1: u8;
    let mask2: u8;
    asm!("in al, dx", out("al") mask1, in("dx") PIC1_DATA);
    unsafe { io_wait() };
    asm!("in al, dx", out("al") mask2, in("dx") PIC2_DATA);
    unsafe { io_wait() };
    unsafe{
    SERIAL_PORT.write_str("  Saved PIC masks - Master: 0x");
    SERIAL_PORT.write_hex(mask1 as u32);
    SERIAL_PORT.write_str(", Slave: 0x");
    SERIAL_PORT.write_hex(mask2 as u32);
    SERIAL_PORT.write_str("\n");
    }

    // Start initialization sequence (ICW1)
    asm!("out dx, al", in("dx") PIC1_COMMAND, in("al") 0x11u8); // Edge-triggered, cascade, ICW4 needed
    unsafe { io_wait() };
    asm!("out dx, al", in("dx") PIC2_COMMAND, in("al") 0x11u8);
    unsafe { io_wait() };
    
    // Set vector offsets (ICW2)
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") 0x20u8); // IRQ0-7 -> ISR32-39
    unsafe { io_wait() };
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") 0x28u8); // IRQ8-15 -> ISR40-47
    unsafe { io_wait() };
    
    // Set cascading (ICW3)
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") 0x04u8); // Master has slave on IRQ2
    unsafe { io_wait() };
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") 0x02u8); // Slave ID
    unsafe { io_wait() };
    
    // Set 8086 mode (ICW4)
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") 0x01u8); // 8086 mode
    unsafe { io_wait() };
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") 0x01u8);
    unsafe { io_wait() };
    
    // Restore masks
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") mask1);
    unsafe { io_wait() };
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") mask2);
    unsafe { io_wait() };

    // Log completion
    unsafe { SERIAL_PORT.write_str("  PIC initialized - Master vector: 0x20, Slave vector: 0x28\n") };
}