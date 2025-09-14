use core::mem::size_of;
use core::ptr::addr_of;
use core::arch::asm;

// IDT entry structure
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    zero: u8,
    flags: u8,
    offset_high: u16,
}

impl IdtEntry {
    const fn new() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            zero: 0,
            flags: 0,
            offset_high: 0,
        }
    }
    
    fn set_handler(&mut self, handler: unsafe extern "C" fn(), selector: u16, flags: u8) {
        let offset = handler as u32;
        self.offset_low = (offset & 0xFFFF) as u16;
        self.offset_high = ((offset >> 16) & 0xFFFF) as u16;
        self.selector = selector;
        self.zero = 0;
        self.flags = flags;
    }
}

// IDT descriptor (pointer)
#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base: u32,
}

// The actual IDT
static mut IDT: [IdtEntry; 256] = [IdtEntry::new(); 256];

// External assembly interrupt handlers - must be marked unsafe
unsafe extern "C" {
    fn isr8();   // Double fault
    fn isr13();  // General protection fault
    fn isr32();  // Timer (IRQ0)
    fn isr33();  // Keyboard (IRQ1)
    fn default_isr();
}

pub fn init() {
    unsafe {
        // Set up exception handlers (ISR 0-31)
        IDT[8].set_handler(isr8, 0x08, 0x8E);   // Double fault
        IDT[13].set_handler(isr13, 0x08, 0x8E); // GPF
        
        // Set up default handlers for other exceptions
        for i in 0..32 {
            if i != 8 && i != 13 {
                IDT[i].set_handler(default_isr, 0x08, 0x8E);
            }
        }
        
        // Set up IRQ handlers (ISR 32-47)
        IDT[32].set_handler(isr32, 0x08, 0x8E); // Timer
        IDT[33].set_handler(isr33, 0x08, 0x8E); // Keyboard
        
        // Set up default handlers for other IRQs
        for i in 34..48 {
            IDT[i].set_handler(default_isr, 0x08, 0x8E);
        }
        
        // Load the IDT
        let idt_desc = IdtDescriptor {
            limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: addr_of!(IDT) as u32,
        };
        
        asm!("lidt [{}]", in(reg) &idt_desc);
    }
}