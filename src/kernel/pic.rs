use core :: arch :: asm;
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const PIC_EOI: u8 = 0x20;

/// Send End-of-Interrupt to PIC
pub unsafe fn send_eoi(irq: u8) {
    if irq >= 8 {
        // Send to slave PIC
        asm!("out dx, al", in("dx") PIC2_COMMAND, in("al") PIC_EOI);
    }
    // Always send to master PIC
    asm!("out dx, al", in("dx") PIC1_COMMAND, in("al") PIC_EOI);
}

/// Initialize the PIC
pub unsafe fn init() {
    // Save masks
    let mask1: u8;
    let mask2: u8;
    asm!("in al, dx", out("al") mask1, in("dx") PIC1_DATA);
    asm!("in al, dx", out("al") mask2, in("dx") PIC2_DATA);
    
    // Start initialization sequence
    asm!("out dx, al", in("dx") PIC1_COMMAND, in("al") 0x11u8);
    asm!("out dx, al", in("dx") PIC2_COMMAND, in("al") 0x11u8);
    
    // Set vector offsets
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") 0x20u8); // IRQ0-7 -> ISR32-39
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") 0x28u8); // IRQ8-15 -> ISR40-47
    
    // Set cascading
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") 0x04u8);
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") 0x02u8);
    
    // Set 8086 mode
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") 0x01u8);
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") 0x01u8);
    
    // Restore masks
    asm!("out dx, al", in("dx") PIC1_DATA, in("al") mask1);
    asm!("out dx, al", in("dx") PIC2_DATA, in("al") mask2);
}