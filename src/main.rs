#![no_std]
#![no_main]
mod multiboot;
mod multiboot_parser;
mod framebuffer_draw;
mod mem;
mod kernel;

use core::panic::PanicInfo;
use core::arch::asm;
use kernel::loggers;
use kernel::loggers::LOGGER;
use kernel::serial::SERIAL_PORT;
use kernel::fb_console;
use kernel::idt;
use kernel::pic;
use kernel::ports;
use kernel:: interupts;
use multiboot_parser::find_framebuffer;


#[inline]
fn current_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}


// Safer version of your _start function with step-by-step debugging

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let magic: u32;
    let info_ptr: u32;
    
    // Read registers
    unsafe {
        asm!(
            "mov {0:e}, eax",
            "mov {1:e}, ebx", 
            out(reg) magic,
            out(reg) info_ptr,
            options(nostack)
        );
    }
    
    unsafe {
        SERIAL_PORT.write_str("=== KERNEL BOOT ===\n");
        SERIAL_PORT.write_str("Magic: 0x");
        SERIAL_PORT.write_hex(magic);
        SERIAL_PORT.write_str("\n");
        LOGGER.info("_start started");
    }

    // Verify multiboot magic
    if magic != 0x36d76289 {
        unsafe {
            SERIAL_PORT.write_str("Invalid multiboot magic!\n");
            panic!("Multiboot magic check failed");
        }
    }

    unsafe {
        SERIAL_PORT.write_str("Multiboot magic OK\n");
        LOGGER.info("Initializing framebuffer");
    }
    
    // Initialize framebuffer
    let fb_opt = unsafe { multiboot_parser::find_framebuffer(info_ptr) };
    
    if let Some(fb) = fb_opt {
        unsafe {
            SERIAL_PORT.write_str("Framebuffer found, setting up graphics\n");
            
            if fb.bpp == 32 {
                fb.draw_gradient();
                fb.fill_rect(20, 20, fb.width - 40, fb.height - 40, 0xFF_00_80_00);
                fb.draw_line(0, 0, (fb.width-1) as isize, (fb.height-1) as isize, 0xFF_FF_00_00);
                fb.draw_line((fb.width-1) as isize, 0, 0, (fb.height-1) as isize, 0xFF_00_FF_00);
            } else {
                fb.clear_32(0xFF_20_20_40);
            }
            
            let mut console = unsafe { fb_console::Console::new(fb, 0xFFFFFFFF, 0xFF000000) };
            console.clear();
            console.put_str("OxideOS Booting...\n");
            console.put_str("Interrupt system initializing\n");
            
            SERIAL_PORT.write_str("Framebuffer setup complete\n");
        }
    }

    // Declare ISR symbols
    unsafe extern "C" {
        fn isr32();
        fn isr13(); 
        fn isr8();
        fn default_isr();
    }

    unsafe {
        SERIAL_PORT.write_str("=== INTERRUPT SETUP ===\n");
        
        // Step 1: Get current CS
        let cs = current_cs();
        SERIAL_PORT.write_str("Current CS: 0x");
        SERIAL_PORT.write_hex(cs as u32);
        SERIAL_PORT.write_str("\n");
        
        // Step 2: Setup IDT entries
        SERIAL_PORT.write_str("Setting up IDT entries\n");
        
        const INT_GATE_ATTR: u8 = 0x8E;
        
        // Install critical exception handlers
        idt::set_idt_entry(8, isr8, cs, INT_GATE_ATTR);     // Double Fault
        idt::set_idt_entry(13, isr13, cs, INT_GATE_ATTR);   // GPF
        
        // Install default handler for most vectors
        for i in 0..32 {
            if i != 8 && i != 13 {
                idt::set_idt_entry(i, default_isr, cs, INT_GATE_ATTR);
            }
        }
        
        // Install timer handler
        idt::set_idt_entry(32, isr32, cs, INT_GATE_ATTR);
        
        SERIAL_PORT.write_str("IDT entries installed\n");
        
        // Step 3: Load IDT
        SERIAL_PORT.write_str("Loading IDT\n");
        idt::load_idt();
        SERIAL_PORT.write_str("IDT loaded successfully\n");
        
        // Step 4: Setup PIC
        SERIAL_PORT.write_str("Remapping PIC\n");
        pic::remap(0x20, 0x28);
        SERIAL_PORT.write_str("PIC remapped\n");
        
        // Step 5: Mask all IRQs initially  
        SERIAL_PORT.write_str("Masking all IRQs\n");
        ports::outb(0x21, 0xFF);
        ports::outb(0xA1, 0xFF);
        
        // Step 6: Test without enabling timer first
        SERIAL_PORT.write_str("=== TESTING PHASE ===\n");
        SERIAL_PORT.write_str("Enabling interrupts WITHOUT timer\n");
        
        core::arch::asm!("sti");
        
        // Wait a bit to see if we get any unexpected interrupts
        for i in 0..1000000 {
            core::arch::asm!("nop");
        }
        
        SERIAL_PORT.write_str("No unexpected interrupts - good!\n");
        
        // Step 7: Enable timer IRQ
        SERIAL_PORT.write_str("Enabling timer IRQ\n");
        let master_mask = ports::inb(0x21) & !(1 << 0);
        ports::outb(0x21, master_mask);
        
        SERIAL_PORT.write_str("Timer IRQ enabled - interrupts should start\n");
        SERIAL_PORT.write_str("=== INTERRUPT SYSTEM ACTIVE ===\n");
    }

    // Main kernel loop
    let mut loop_count = 0;
    loop {
        unsafe {
            if loop_count % 10000000 == 0 {
                SERIAL_PORT.write_str("Main loop iteration: ");
                SERIAL_PORT.write_decimal(loop_count / 10000000);
                SERIAL_PORT.write_str("\n");
            }
            loop_count += 1;
            
            core::arch::asm!("hlt");
        }
    }
}


#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        LOGGER.error("KERNEL PANIC OCCURRED!");
        
        if let Some(location) = info.location() {
            SERIAL_PORT.write_str("Location: ");
            SERIAL_PORT.write_str(location.file());
            SERIAL_PORT.write_str(":");
            SERIAL_PORT.write_decimal(location.line());
            SERIAL_PORT.write_str(":");
            SERIAL_PORT.write_decimal(location.column());
            SERIAL_PORT.write_str("\n");
        }
        
        // info.message() returns PanicMessage directly, not Option<PanicMessage>
        let msg = info.message();
        SERIAL_PORT.write_str("Message: ");
        SERIAL_PORT.write_str("(message formatting not implemented)\n");
        
        LOGGER.error("System halted due to panic");
    }
    
    // Disable interrupts and halt
    unsafe {
        asm!("cli");
        loop {
            asm!("hlt");
        }
    }
}
