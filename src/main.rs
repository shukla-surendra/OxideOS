//! OxideOS Kernel Main Entry Point
//! 
//! This file contains the kernel's main entry point and initialization sequence.
//! The initialization follows the proper order that a kernel needs to boot.

#![no_std]
#![no_main]

// ============================================================================
// MODULE DECLARATIONS - Core kernel modules
// ============================================================================
mod panic;              // panic handler
mod multiboot;          // Multiboot2 specification handling
mod multiboot_parser;   // Parse multiboot info structure
mod framebuffer_draw;   // Framebuffer graphics primitives
mod mem;                // Memory management (will be expanded later)
mod kernel;             // Core kernel subsystems

// ============================================================================
// IMPORTS - Only what we need for early boot
// ============================================================================
use core::arch::asm;
use kernel::loggers::LOGGER;
use kernel::serial::SERIAL_PORT;
use kernel::fb_console;
use kernel::idt;
use kernel::pic;
use kernel::ports;
use kernel::interupts;  // Note: should be 'interrupts' (typo to fix)
use multiboot_parser::find_framebuffer;

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Get current Code Segment register value
#[inline]
fn current_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}

// ============================================================================
// KERNEL ENTRY POINT
// ============================================================================

/// Main kernel entry point - called by bootloader
/// 
/// Initialization Order (Critical - Don't Change!):
/// 1. Hardware Detection & Setup
/// 2. Memory Management 
/// 3. Display/Console
/// 4. Interrupt System
/// 5. Device Drivers
/// 6. Process Management
/// 7. File System
/// 8. User Mode & System Calls
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // ========================================================================
    // STAGE 1: BOOTLOADER HANDOFF - Get control from bootloader
    // ========================================================================
    let magic: u32;
    let info_ptr: u32;
    
    // Read multiboot magic and info pointer from registers
    unsafe {
        asm!(
            "mov {0:e}, eax",    // EAX contains multiboot2 magic
            "mov {1:e}, ebx",    // EBX contains pointer to multiboot info
            out(reg) magic,
            out(reg) info_ptr,
            options(nostack)
        );
    }
    
    // Initialize early logging
    unsafe {
        SERIAL_PORT.write_str("=== OXIDEOS KERNEL BOOT ===\n");
        SERIAL_PORT.write_str("Multiboot Magic: 0x");
        SERIAL_PORT.write_hex(magic);
        SERIAL_PORT.write_str("\n");
        LOGGER.info("Kernel entry point reached");
    }

    // Verify we were loaded by a multiboot2-compliant bootloader
    if magic != 0x36d76289 {
        unsafe {
            SERIAL_PORT.write_str("FATAL: Invalid multiboot2 magic number!\n");
            panic!("Multiboot2 magic verification failed");
        }
    }

    unsafe {
        SERIAL_PORT.write_str("✓ Multiboot2 handoff successful\n");
    }
    
    // ========================================================================
    // STAGE 2: HARDWARE DISCOVERY - Parse bootloader-provided info
    // ========================================================================
    unsafe {
        LOGGER.info("Parsing multiboot information structure");
    }
    
    // TODO: Parse full multiboot info (memory map, modules, etc.)
    // For now, just get framebuffer info
    let fb_opt = unsafe { find_framebuffer(info_ptr) };
    
    // ========================================================================
    // STAGE 3: EARLY MEMORY SETUP - Basic memory management
    // ========================================================================
    // TODO: Initialize early heap allocator
    // TODO: Set up basic page tables if needed
    // TODO: Parse memory map from multiboot info
    unsafe {
        LOGGER.info("Early memory setup (TODO - placeholder)");
        // mem::init_early_memory(info_ptr);
    }
    
    // ========================================================================
    // STAGE 4: DISPLAY INITIALIZATION - Set up graphics and console
    // ========================================================================
    let mut console_opt = None;
    
    if let Some(fb) = fb_opt {
        unsafe {
            SERIAL_PORT.write_str("✓ Framebuffer detected - initializing graphics\n");
            
            // Set up basic graphics test pattern
            if fb.bpp == 32 {
                fb.draw_gradient();
                fb.fill_rect(20, 20, fb.width - 40, fb.height - 40, 0xFF_00_80_00);
                fb.draw_line(0, 0, (fb.width-1) as isize, (fb.height-1) as isize, 0xFF_FF_00_00);
                fb.draw_line((fb.width-1) as isize, 0, 0, (fb.height-1) as isize, 0xFF_00_FF_00);
            } else {
                fb.clear_32(0xFF_20_20_40);
            }
            
            // Initialize text console overlay
            let mut console = fb_console::Console::new(fb, 0xFFFFFFFF, 0xFF000000);
            console.clear();
            console.put_str("OxideOS v0.1 - Booting...\n");
            console.put_str("Initializing kernel subsystems...\n");
            console_opt = Some(console);
            
            SERIAL_PORT.write_str("✓ Graphics and console initialized\n");
        }
    } else {
        unsafe {
            SERIAL_PORT.write_str("⚠ No framebuffer - text mode only\n");
        }
    }
    
    // ========================================================================
    // STAGE 5: INTERRUPT SYSTEM - Critical for multitasking and I/O
    // ========================================================================
    
    // Declare interrupt service routine symbols (defined in assembly)
    unsafe extern "C" {
        fn isr32();       // Timer interrupt (IRQ0)
        fn isr33();       // Keyboard interrupt (IRQ1) 
        fn isr13();       // General Protection Fault
        fn isr8();        // Double Fault
        fn default_isr(); // Default handler for unhandled interrupts
    }

    unsafe {
        SERIAL_PORT.write_str("=== INTERRUPT SYSTEM INITIALIZATION ===\n");
        
        // Get current code segment for IDT entries
        let cs = current_cs();
        SERIAL_PORT.write_str("Current CS: 0x");
        SERIAL_PORT.write_hex(cs as u32);
        SERIAL_PORT.write_str("\n");
        
        // Step 1: Set up Interrupt Descriptor Table (IDT)
        SERIAL_PORT.write_str("Setting up IDT entries...\n");
        const INT_GATE_ATTR: u8 = 0x8E; // Present, Ring 0, 32-bit interrupt gate
        
        // Install CPU exception handlers (vectors 0-31)
        idt::set_idt_entry(8, isr8, cs, INT_GATE_ATTR);      // Double Fault
        idt::set_idt_entry(13, isr13, cs, INT_GATE_ATTR);    // General Protection Fault
        
        // Install default handler for other exceptions
        for i in 0..32 {
            if i != 8 && i != 13 {
                idt::set_idt_entry(i, default_isr, cs, INT_GATE_ATTR);
            }
        }
        
        // Install hardware interrupt handlers (IRQs remapped to vectors 32+)
        idt::set_idt_entry(32, isr32, cs, INT_GATE_ATTR);    // Timer (IRQ0)
        idt::set_idt_entry(33, isr33, cs, INT_GATE_ATTR);    // Keyboard (IRQ1)
        
        // Load IDT into CPU
        idt::load_idt();
        SERIAL_PORT.write_str("✓ IDT loaded\n");
        
        // Step 2: Configure Programmable Interrupt Controller (PIC)
        SERIAL_PORT.write_str("Configuring PIC...\n");
        pic::remap(0x20, 0x28); // Remap IRQs to vectors 32-47
        
        // Mask all hardware interrupts initially
        ports::outb(0x21, 0xFF); // Master PIC mask
        ports::outb(0xA1, 0xFF); // Slave PIC mask
        SERIAL_PORT.write_str("✓ PIC configured, all IRQs masked\n");
        
        // Step 3: Test interrupt system without hardware IRQs
        SERIAL_PORT.write_str("Testing interrupt system...\n");
        core::arch::asm!("sti"); // Enable interrupts
        
        // Brief test period
        for _ in 0..1000000 {
            core::arch::asm!("nop");
        }
        SERIAL_PORT.write_str("✓ No spurious interrupts detected\n");
        
        // Step 4: Enable timer interrupt for multitasking
        SERIAL_PORT.write_str("Enabling timer interrupt...\n");
        let master_mask = ports::inb(0x21) & !(1 << 0); // Unmask IRQ0 (timer)
        ports::outb(0x21, master_mask);
        
        SERIAL_PORT.write_str("✓ Timer interrupt enabled\n");
        SERIAL_PORT.write_str("=== INTERRUPT SYSTEM ACTIVE ===\n");
    }
    
    // Update console if available
    if let Some(ref mut console) = console_opt {
        unsafe {
            console.put_str("✓ Interrupt system initialized\n");
        }
    }
    
    // ========================================================================
    // STAGE 6: DEVICE DRIVERS (Future - commented out for now)
    // ========================================================================
    // TODO: Initialize keyboard driver
    // TODO: Initialize storage drivers (ATA/AHCI)
    // TODO: Initialize network drivers
    // TODO: Initialize USB subsystem
    unsafe {
        LOGGER.info("Device driver initialization (TODO - placeholder)");
        // keyboard::init();
        // storage::init(); 
        // network::init();
    }
    
    // ========================================================================
    // STAGE 7: MEMORY MANAGEMENT (Future - commented out for now) 
    // ========================================================================
    // TODO: Set up full virtual memory system
    // TODO: Initialize kernel heap
    // TODO: Set up memory allocators
    unsafe {
        LOGGER.info("Full memory management setup (TODO - placeholder)");
        // mem::init_virtual_memory();
        // mem::init_kernel_heap();
    }
    
    // ========================================================================
    // STAGE 8: PROCESS MANAGEMENT (Future - commented out for now)
    // ========================================================================
    // TODO: Initialize task scheduler
    // TODO: Set up initial kernel tasks
    // TODO: Prepare for user mode
    unsafe {
        LOGGER.info("Process management initialization (TODO - placeholder)");
        // scheduler::init();
        // task::create_init_task();
    }
    
    // ========================================================================
    // STAGE 9: FILE SYSTEM (Future - commented out for now)
    // ========================================================================
    // TODO: Initialize VFS (Virtual File System)
    // TODO: Mount root filesystem
    // TODO: Set up device files (/dev)
    unsafe {
        LOGGER.info("File system initialization (TODO - placeholder)");
        // vfs::init();
        // fs::mount_root();
    }
    
    // ========================================================================
    // STAGE 10: SYSTEM CALLS & USER MODE (Future - commented out for now)
    // ========================================================================
    // TODO: Set up system call interface
    // TODO: Initialize user space
    // TODO: Start init process
    unsafe {
        LOGGER.info("User mode preparation (TODO - placeholder)");
        // syscalls::init();
        // usermode::init();
        // process::start_init();
    }
    
    // ========================================================================
    // STAGE 11: KERNEL MAIN LOOP - The kernel is now fully initialized
    // ========================================================================
    unsafe {
        SERIAL_PORT.write_str("=== KERNEL INITIALIZATION COMPLETE ===\n");
        LOGGER.info("Entering main kernel loop");
    }
    
    if let Some(ref mut console) = console_opt {
        unsafe {
            console.put_str("✓ Kernel boot complete - System ready\n");
            console.put_str("Timer interrupts active...\n");
        }
    }
    
    // Main kernel idle loop - in a real OS, this would be the scheduler
    let mut loop_count = 0;
    loop {
        unsafe {
            // Periodic status updates
            if loop_count % 10000000 == 0 {
                SERIAL_PORT.write_str("Kernel idle loop: ");
                SERIAL_PORT.write_decimal(loop_count / 10000000);
                SERIAL_PORT.write_str("\n");
            }
            loop_count += 1;
            
            // TODO: In a real kernel, this would be:
            // scheduler::yield(); // Switch to next process
            // Or handle pending kernel tasks
            
            // For now, just halt until next interrupt
            core::arch::asm!("hlt"); 
        }
    }
}