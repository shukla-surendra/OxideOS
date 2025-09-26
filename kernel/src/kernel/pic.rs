// src/kernel/pic.rs - 64-bit PIC (Programmable Interrupt Controller) module
use crate::kernel::serial::SERIAL_PORT;
use core::arch::asm;

// PIC ports (same addresses in 64-bit)
pub const PIC1_COMMAND: u16 = 0x20;
pub const PIC1_DATA: u16 = 0x21;
pub const PIC2_COMMAND: u16 = 0xA0;
pub const PIC2_DATA: u16 = 0xA1;
pub const PIC_EOI: u8 = 0x20;

// I/O wait function using port 0x80 (same in 64-bit)
unsafe fn io_wait() {
    asm!(
        "out 0x80, al", 
        in("al") 0u8,
        options(nostack, nomem)
    );
}

pub unsafe fn init() {
    // Save current masks
    let mask1: u8;
    let mask2: u8;
    
    asm!(
        "in al, dx", 
        out("al") mask1, 
        in("dx") PIC1_DATA,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "in al, dx", 
        out("al") mask2, 
        in("dx") PIC2_DATA,
        options(nostack, nomem)
    );
    io_wait();
    
    SERIAL_PORT.write_str("64-bit PIC: Saved masks - Master: 0x");
    SERIAL_PORT.write_hex(mask1 as u32);
    SERIAL_PORT.write_str(", Slave: 0x");
    SERIAL_PORT.write_hex(mask2 as u32);
    SERIAL_PORT.write_str("\n");

    // ICW1: Start initialization sequence
    // 0x11 = Edge-triggered, cascade mode, ICW4 needed
    asm!(
        "out dx, al", 
        in("dx") PIC1_COMMAND, 
        in("al") 0x11u8,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "out dx, al", 
        in("dx") PIC2_COMMAND, 
        in("al") 0x11u8,
        options(nostack, nomem)
    );
    io_wait();
    
    // ICW2: Set vector offsets
    // Master PIC: IRQ0-7 -> Interrupts 32-39
    // Slave PIC: IRQ8-15 -> Interrupts 40-47
    asm!(
        "out dx, al", 
        in("dx") PIC1_DATA, 
        in("al") 0x20u8,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "out dx, al", 
        in("dx") PIC2_DATA, 
        in("al") 0x28u8,
        options(nostack, nomem)
    );
    io_wait();
    
    // ICW3: Set up cascading
    // Master: bit 2 set (IRQ2 has slave)
    // Slave: cascade identity 2
    asm!(
        "out dx, al", 
        in("dx") PIC1_DATA, 
        in("al") 0x04u8,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "out dx, al", 
        in("dx") PIC2_DATA, 
        in("al") 0x02u8,
        options(nostack, nomem)
    );
    io_wait();
    
    // ICW4: Set 8086 mode
    asm!(
        "out dx, al", 
        in("dx") PIC1_DATA, 
        in("al") 0x01u8,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "out dx, al", 
        in("dx") PIC2_DATA, 
        in("al") 0x01u8,
        options(nostack, nomem)
    );
    io_wait();
    
    // Mask all interrupts initially
    asm!(
        "out dx, al", 
        in("dx") PIC1_DATA, 
        in("al") 0xFFu8,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "out dx, al", 
        in("dx") PIC2_DATA, 
        in("al") 0xFFu8,
        options(nostack, nomem)
    );
    io_wait();

    // Clear any pending interrupts
    asm!(
        "out dx, al", 
        in("dx") PIC1_COMMAND, 
        in("al") PIC_EOI,
        options(nostack, nomem)
    );
    io_wait();
    
    asm!(
        "out dx, al", 
        in("dx") PIC2_COMMAND, 
        in("al") PIC_EOI,
        options(nostack, nomem)
    );
    io_wait();

    SERIAL_PORT.write_str("64-bit PIC initialized:\n");
    SERIAL_PORT.write_str("  Master vector: 0x20 (32), Slave vector: 0x28 (40)\n");
    SERIAL_PORT.write_str("  All IRQs masked initially\n");
}

pub unsafe fn send_eoi(irq: u8) {
    // Send EOI to slave PIC if IRQ came from slave (IRQ8-15)
    if irq >= 8 {
        asm!(
            "out dx, al", 
            in("dx") PIC2_COMMAND, 
            in("al") PIC_EOI,
            options(nostack, nomem)
        );
    }
    
    // Always send EOI to master PIC
    asm!(
        "out dx, al", 
        in("dx") PIC1_COMMAND, 
        in("al") PIC_EOI,
        options(nostack, nomem)
    );
}

/// Unmask (enable) a specific IRQ
pub unsafe fn unmask_irq(irq: u8) {
    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let irq_bit = if irq < 8 { irq } else { irq - 8 };
    
    let mut mask: u8;
    asm!(
        "in al, dx", 
        out("al") mask, 
        in("dx") port,
        options(nostack, nomem)
    );
    
    mask &= !(1 << irq_bit);
    
    asm!(
        "out dx, al", 
        in("dx") port, 
        in("al") mask,
        options(nostack, nomem)
    );
    
    SERIAL_PORT.write_str("64-bit PIC: Unmasked IRQ");
    SERIAL_PORT.write_decimal(irq as u32);
    SERIAL_PORT.write_str(" (mask now: 0x");
    SERIAL_PORT.write_hex(mask as u32);
    SERIAL_PORT.write_str(")\n");
}

/// Mask (disable) a specific IRQ
pub unsafe fn mask_irq(irq: u8) {
    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let irq_bit = if irq < 8 { irq } else { irq - 8 };
    
    let mut mask: u8;
    asm!(
        "in al, dx", 
        out("al") mask, 
        in("dx") port,
        options(nostack, nomem)
    );
    
    mask |= 1 << irq_bit;
    
    asm!(
        "out dx, al", 
        in("dx") port, 
        in("al") mask,
        options(nostack, nomem)
    );
}

/// Get current mask for a PIC (master = false, slave = true)
pub unsafe fn get_mask(slave: bool) -> u8 {
    let port = if slave { PIC2_DATA } else { PIC1_DATA };
    let mut mask: u8;
    
    asm!(
        "in al, dx", 
        out("al") mask, 
        in("dx") port,
        options(nostack, nomem)
    );
    
    mask
}