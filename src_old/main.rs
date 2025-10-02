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
use kernel::{fb_console, idt, interrupts, timer, pic};
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

/// Check stack pointer before and after main loop iterations
/// Fixed stack validation based on actual stack location (around 0x7FD54)
unsafe fn check_stack_in_main_loop(iteration: u32) {
    let esp: u32;
    core::arch::asm!("mov {}, esp", out(reg) esp, options(nomem, nostack, preserves_flags));
    
    // Only check stack periodically to avoid spam
    if iteration % 10000000 == 0 {
        SERIAL_PORT.write_str("Main loop ESP: 0x");
        SERIAL_PORT.write_hex(esp);
        
        // Adjusted validation for actual stack location (around 0x7FD54)
        // stack is around 524KB, so let's be more realistic about bounds
        let valid = if esp == 0 {
            SERIAL_PORT.write_str(" **NULL**");
            false
        } else if esp < 0x70000 {  // Below 448KB - too low for setup
            SERIAL_PORT.write_str(" **TOO_LOW**");
            false
        } else if esp > 0x100000 {  // Above 1MB - too high for early boot
            SERIAL_PORT.write_str(" **TOO_HIGH**");
            false
        } else if esp % 4 != 0 {  // Not 4-byte aligned
            SERIAL_PORT.write_str(" **UNALIGNED**");
            false
        } else {
            SERIAL_PORT.write_str(" OK");
            true
        };
        
        SERIAL_PORT.write_str("\n");
        
        if !valid {
            SERIAL_PORT.write_str("STACK CORRUPTION DETECTED in main loop!\n");
            SERIAL_PORT.write_str("ESP: 0x");
            SERIAL_PORT.write_hex(esp);
            SERIAL_PORT.write_str("\n");
            SERIAL_PORT.write_str("Expected range: 0x70000 - 0x100000\n");
            
            core::arch::asm!("cli");
            loop { core::arch::asm!("hlt"); }
        }
    }
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
        
        // Log initial stack pointer
        let initial_esp: u32;
        core::arch::asm!("mov {}, esp", out(reg) initial_esp, options(nomem, nostack, preserves_flags));
        SERIAL_PORT.write_str("Initial ESP: 0x");
        SERIAL_PORT.write_hex(initial_esp);
        SERIAL_PORT.write_str("\n");
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
                // commenting graphic draw to focus on interrupts
                // fb.draw_gradient();
                // fb.fill_rect(20, 20, fb.width - 40, fb.height - 40, 0xFF_00_80_00);
                // fb.draw_line(0, 0, (fb.width-1) as isize, (fb.height-1) as isize, 0xFF_FF_00_00);
                // fb.draw_line((fb.width-1) as isize, 0, 0, (fb.height-1) as isize, 0xFF_00_FF_00);
            } else {
                // fb.clear_32(0xFF_20_20_40);
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
unsafe {
        SERIAL_PORT.write_str("Step 5: Initializing PIC...\n");
        pic::init();
        SERIAL_PORT.write_str("  ✓ PIC remapped (IRQ0-7 -> ISR32-39)\n");
        
        SERIAL_PORT.write_str("Step 6: Initializing PIT timer...\n");
        timer::init(100); // 100 Hz
        SERIAL_PORT.write_str("  ✓ PIT timer initialized at 100Hz\n");

        // Verify IDT entries before enabling interrupts
        SERIAL_PORT.write_str("Step 7: Verifying IDT entries...\n");
        verify_idt_entries();

        // Testing PIC mapping
        SERIAL_PORT.write_str("  Testing PIC mapping by unmasking IRQ0 briefly...\n");
        
        // Check stack alignment before enabling interrupts
        let esp: u32;
        asm!("mov {}, esp", out(reg) esp, options(nomem, nostack, preserves_flags));
        SERIAL_PORT.write_str("  Pre-IRQ0 ESP: 0x");
        SERIAL_PORT.write_hex(esp);
        if esp % 4 != 0 {
            SERIAL_PORT.write_str(" **MISALIGNED**");
            SERIAL_PORT.write_str("\n  WARNING: Stack misaligned before enabling IRQ0! Continuing...\n");
        } else {
            SERIAL_PORT.write_str(" OK\n");
        }

        // Check EFLAGS before enabling interrupts
        let eflags: u32;
        asm!("pushf; pop {}", out(reg) eflags, options(nomem, nostack));
        SERIAL_PORT.write_str("  Pre-IRQ0 EFLAGS: 0x");
        SERIAL_PORT.write_hex(eflags);
        if (eflags & (1 << 9)) == 0 {
            SERIAL_PORT.write_str(" (IF=0, interrupts disabled)");
        } else {
            SERIAL_PORT.write_str(" (IF=1, interrupts enabled)");
        }
        SERIAL_PORT.write_str("\n");

        // Enable interrupts and unmask IRQ0 only
        asm!("sti");
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFEu8); // Only IRQ0 enabled
        let mut current_ticks = timer::get_ticks();
        let mut iterations: u32 = 0;

        while timer::get_ticks() - current_ticks < 10 && iterations < 100000000 {
            iterations += 1;
            if iterations % 10000000 == 0 {
                SERIAL_PORT.write_str("  Waiting for ticks: current = ");
                SERIAL_PORT.write_decimal(timer::get_ticks() as u32);
                SERIAL_PORT.write_str("\n");
            }
            asm!("hlt"); // Wait for interrupt
        }

        let final_ticks = timer::get_ticks() - current_ticks;
        if final_ticks >= 10 {
            SERIAL_PORT.write_str("  SUCCESS: Timer interrupts working! Ticks: ");
            SERIAL_PORT.write_decimal(final_ticks as u32);
            SERIAL_PORT.write_str("\n");
        } else {
            SERIAL_PORT.write_str("  WARNING: Timer test timed out after ");
            SERIAL_PORT.write_decimal(iterations);
            SERIAL_PORT.write_str(" iterations. Ticks: ");
            SERIAL_PORT.write_decimal(final_ticks as u32);
            SERIAL_PORT.write_str("\n");
        }

        // Mask IRQ0 again to stop timer for now
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFFu8);
    }
    unsafe {
        check_system_tables();
        init_interrupts();
    }
    // ========================================================================
    // STAGE 6: DEVICE DRIVERS (Future - commented out for now)
    // ========================================================================
    unsafe {
        LOGGER.info("Device driver initialization (TODO - placeholder)");
    }
    
    // ========================================================================
    // STAGE 7: MEMORY MANAGEMENT (Future - commented out for now) 
    // ========================================================================
    unsafe {
        LOGGER.info("Full memory management setup (TODO - placeholder)");
    }
    
    // ========================================================================
    // STAGE 8: PROCESS MANAGEMENT (Future - commented out for now)
    // ========================================================================
    unsafe {
        LOGGER.info("Process management initialization (TODO - placeholder)");
    }
    
    // ========================================================================
    // STAGE 9: FILE SYSTEM (Future - commented out for now)
    // ========================================================================
    unsafe {
        LOGGER.info("File system initialization (TODO - placeholder)");
    }
    
    // ========================================================================
    // STAGE 10: SYSTEM CALLS & USER MODE (Future - commented out for now)
    // ========================================================================
    unsafe {
        LOGGER.info("User mode preparation (TODO - placeholder)");
    }
    
    // ========================================================================
    // STAGE 11: KERNEL MAIN LOOP
    // ========================================================================
    unsafe {
        SERIAL_PORT.write_str("=== KERNEL INITIALIZATION COMPLETE ===\n");
        SERIAL_PORT.write_str("Entering main kernel loop\n");
        
        // Log final stack pointer before entering main loop
        let final_esp: u32;
        core::arch::asm!("mov {}, esp", out(reg) final_esp, options(nomem, nostack, preserves_flags));
        SERIAL_PORT.write_str("Final ESP before main loop: 0x");
        SERIAL_PORT.write_hex(final_esp);
        SERIAL_PORT.write_str("\n");
    }
    
    if let Some(ref mut console) = console_opt {
        unsafe{
            console.put_str("✓ Kernel boot complete - System ready\n");
            console.put_str("Keyboard interrupts active...\n");

        }

    }
    
    // Main kernel idle loop
    let mut last_second = 0;
    let mut loop_counter = 0u32;
    
    unsafe {
        SERIAL_PORT.write_str("Starting main loop...\n");
    }
    
    loop {
        loop_counter = loop_counter.wrapping_add(1);
        
        // Check timer periodically
        let ticks = unsafe { timer::get_ticks() };
        let seconds = ticks / 100;  // Assuming 100Hz timer
        
        if seconds != last_second {
            last_second = seconds;
            unsafe {
                SERIAL_PORT.write_str("Uptime: ");
                SERIAL_PORT.write_decimal(seconds as u32);
                SERIAL_PORT.write_str(" seconds (ticks: ");
                SERIAL_PORT.write_decimal(ticks as u32);
                SERIAL_PORT.write_str(")\n");
            }
        }
        
        // Periodic health check (every ~10M iterations)
        if loop_counter % 10_000_000 == 0 {
            unsafe {
                check_stack_in_main_loop(loop_counter);
            }
        }
        
        // Halt until next interrupt
        unsafe { 
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn init_interrupts() {
    unsafe {
        // Disable APIC
        SERIAL_PORT.write_str("Disabling APIC...\n");
        let apic_base: u32;
        asm!("rdmsr", in("ecx") 0x1B, out("eax") apic_base, out("edx") _); // Read APIC base MSR
        asm!("wrmsr", in("ecx") 0x1B, in("eax") (apic_base & !(1 << 11)), in("edx") 0); // Clear APIC enable bit
        SERIAL_PORT.write_str("  APIC disabled\n");
        // 1. Disable interrupts during setup
        SERIAL_PORT.write_str("Step 1: Disabling interrupts (CLI)...\n");
        asm!("cli");
        
        // 2. Verify handler addresses before IDT setup
        SERIAL_PORT.write_str("Step 2: Verifying ISR addresses...\n");
        interrupts::verify_handlers();
        
        // 3. Initialize the IDT
        SERIAL_PORT.write_str("Step 3: Initializing IDT...\n");
        idt::init();
        SERIAL_PORT.write_str("  ✓ IDT loaded\n");
        
        // 4. Skip IDT entry verification for now - it's causing the panic
        SERIAL_PORT.write_str("Step 4:verify_idt_entries\n");
        verify_idt_entries();
        
        // Step 5: Initializing PIC
        SERIAL_PORT.write_str("Step 5: Initializing PIC...\n");
        pic::init();
        SERIAL_PORT.write_str("  ✓ PIC remapped (IRQ0-7 -> ISR32-39)\n");
        
        // Test IRQ0 mapping
        SERIAL_PORT.write_str("  Testing PIC mapping by unmasking IRQ0 briefly...\n");
        asm!("sti"); // Enable interrupts
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFEu8); // Enable only IRQ0
        for _ in 0..1000000 { // Longer loop to ensure interrupt fires
            asm!("nop");
        }
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFFu8); // Mask IRQ0 again
        asm!("cli"); // Disable interrupts
        SERIAL_PORT.write_str("  IRQ0 test complete\n");

        // Step 6: Masking all interrupts
        SERIAL_PORT.write_str("Step 6: Masking all interrupts initially...\n");
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFFu8); // Mask all on master PIC
        asm!("out dx, al", in("dx") 0xA1u16, in("al") 0xFFu8); // Mask all on slave PIC
        
        // 7. Configure the timer
        SERIAL_PORT.write_str("Step 7: Configuring timer (100Hz)...\n");
        timer::init(100);
        SERIAL_PORT.write_str("  ✓ Timer configured\n");
        
        // 8. Enable interrupts globally
        SERIAL_PORT.write_str("  Enabling interrupts (STI)...\n");
        asm!("sti");
        SERIAL_PORT.write_str("  Unmasking only IRQ0 (timer)...\n");
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFEu8); // Only IRQ0 enabled
        
        // 9. Test basic exception handling with INT3
        SERIAL_PORT.write_str("Step 9: Testing exception handling with INT3 (breakpoint)...\n");
        // test_int3_exception();
        
        // 10. Enable keyboard interrupt only
        // SERIAL_PORT.write_str("Step 10: Enabling ONLY keyboard interrupt (IRQ1)...\n");
        // asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFDu8);  // Only IRQ1 enabled
        // SERIAL_PORT.write_str("  ✓ Keyboard enabled, press a key to test\n");
        
        // Wait for keyboard test
        // for _ in 0..10000000 {
        //     asm!("nop");
        // }
        
        // Step 11: Enable timer (IRQ0) - with better debugging
        // SERIAL_PORT.write_str("Step 11: Now enabling timer (IRQ0)...\n");
        
        // First, let's see current timer ticks (should be 0)
        let initial_ticks = timer::get_ticks();
        SERIAL_PORT.write_str("  Initial timer ticks: ");
        SERIAL_PORT.write_decimal(initial_ticks as u32);
        SERIAL_PORT.write_str("\n");
        
        // Enable only timer for now (mask = 0xFE = 11111110 binary = all masked except IRQ0)
        // SERIAL_PORT.write_str("  Unmasking only IRQ0 (timer)...\n");
        // asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFEu8); // Only IRQ0 enabled
        
        // Wait for exactly 10 timer interrupts
        SERIAL_PORT.write_str("  Waiting for timer interrupts...\n");
        let target_ticks = initial_ticks + 10;
        let mut iterations = 0u32;
        
        loop {
            let current_ticks = timer::get_ticks();
            if current_ticks >= target_ticks {
                SERIAL_PORT.write_str("  SUCCESS: Timer interrupts working! Ticks: ");
                SERIAL_PORT.write_decimal(current_ticks as u32);
                SERIAL_PORT.write_str("\n");
                break;
            }
            
            iterations += 1;
            if iterations > 10_000_000 {
                SERIAL_PORT.write_str("  TIMEOUT: No timer interrupts received after 10M iterations\n");
                SERIAL_PORT.write_str("  Current ticks: ");
                SERIAL_PORT.write_decimal(current_ticks as u32);
                SERIAL_PORT.write_str("\n");
                break;
            }
            
            // Small delay
            for _ in 0..100 {
                asm!("nop");
            }
        }
        
        // Now enable keyboard too
        SERIAL_PORT.write_str("  Now enabling keyboard (IRQ1) as well...\n");
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xFCu8); // IRQ0 and IRQ1 enabled
        SERIAL_PORT.write_str("  Both timer and keyboard enabled\n");
    }
}

// Test INT3 (breakpoint) exception - this is safer than divide by zero
fn test_int3_exception() {
    unsafe {
        SERIAL_PORT.write_str("  Triggering INT3 (breakpoint)...\n");
        
        // INT3 is a single-byte instruction that triggers exception 3
        asm!("int3");
        
        // If we get here, the handler returned (which shouldn't happen for exceptions)
        SERIAL_PORT.write_str("  WARNING: INT3 handler returned (should have halted)\n");
    }
}

// Verify IDT entries are set correctly
// Enhanced safer IDT verification to avoid null pointer panic
fn verify_idt_entries() {
    unsafe {
        let mut idtr: [u8; 6] = [0; 6];
        asm!("sidt [{}]", in(reg) &mut idtr);
        let idt_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
        let idt_base = u32::from_le_bytes([idtr[2], idtr[3], idtr[4], idtr[5]]);
        
        SERIAL_PORT.write_str("  IDT Base: 0x");
        SERIAL_PORT.write_hex(idt_base);
        SERIAL_PORT.write_str(", Limit: 0x");
        SERIAL_PORT.write_hex(idt_limit as u32);
        SERIAL_PORT.write_str("\n");

        if idt_base == 0 {
            SERIAL_PORT.write_str("  ERROR: IDT base is null! Cannot verify entries.\n");
            return;
        }

        // Check entry 0 (divide by zero)
        let idt_ptr = idt_base as *const u64;
        let entry0 = *idt_ptr.offset(0);
        let offset0 = ((entry0 & 0xFFFF) | ((entry0 >> 32) & 0xFFFF0000)) as u32;
        SERIAL_PORT.write_str("  IDT[0] offset: 0x");
        SERIAL_PORT.write_hex(offset0);
        
        // Check entry 32 (timer)
        let entry32 = *idt_ptr.offset(32);
        let offset32 = ((entry32 & 0xFFFF) | ((entry32 >> 32) & 0xFFFF0000)) as u32;
        SERIAL_PORT.write_str("\n  IDT[32] offset: 0x");
        SERIAL_PORT.write_hex(offset32);
        // Compare with actual isr32 address
        let isr32_addr = interrupts::isr32 as usize as u32;
        SERIAL_PORT.write_str(" (isr32: 0x");
        SERIAL_PORT.write_hex(isr32_addr);
        SERIAL_PORT.write_str(")");
        if offset32 != isr32_addr {
            SERIAL_PORT.write_str(" **MISMATCH**");
        } else {
            SERIAL_PORT.write_str(" OK");
        }
        
        // Check entry 33 (keyboard)
        let entry33 = *idt_ptr.offset(33);
        let offset33 = ((entry33 & 0xFFFF) | ((entry33 >> 32) & 0xFFFF0000)) as u32;
        SERIAL_PORT.write_str("\n  IDT[33] offset: 0x");
        SERIAL_PORT.write_hex(offset33);
        SERIAL_PORT.write_str("\n");
    }
}
// Also add this diagnostic function to check GDT/IDT state:
// Fix GDT entry print in check_system_tables
unsafe fn check_system_tables() {
    unsafe {
        SERIAL_PORT.write_str("\n=== SYSTEM TABLE CHECK ===\n");
        
        // Check GDT
        let mut gdt_ptr: [u8; 6] = [0; 6];
        asm!("sgdt [{}]", in(reg) &mut gdt_ptr);
        let gdt_limit = u16::from_le_bytes([gdt_ptr[0], gdt_ptr[1]]);
        let gdt_base = u32::from_le_bytes([gdt_ptr[2], gdt_ptr[3], gdt_ptr[4], gdt_ptr[5]]);
        
        SERIAL_PORT.write_str("GDT Base: 0x");
        SERIAL_PORT.write_hex(gdt_base);
        SERIAL_PORT.write_str(", Limit: 0x");
        SERIAL_PORT.write_hex(gdt_limit as u32);
        SERIAL_PORT.write_str("\n");
        
        // Check IDT
        let mut idt_ptr: [u8; 6] = [0; 6];
        asm!("sidt [{}]", in(reg) &mut idt_ptr);
        let idt_limit = u16::from_le_bytes([idt_ptr[0], idt_ptr[1]]);
        let idt_base = u32::from_le_bytes([idt_ptr[2], idt_ptr[3], idt_ptr[4], idt_ptr[5]]);
        
        SERIAL_PORT.write_str("IDT Base: 0x");
        SERIAL_PORT.write_hex(idt_base);
        SERIAL_PORT.write_str(", Limit: 0x");
        SERIAL_PORT.write_hex(idt_limit as u32);
        SERIAL_PORT.write_str("\n");
        
        // Check CS register
        let cs: u16;
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
        SERIAL_PORT.write_str("CS: 0x");
        SERIAL_PORT.write_hex(cs as u32);
        SERIAL_PORT.write_str("\n");

        let gdt_base_ptr = gdt_base as *const u64;
        let cs_entry = *gdt_base_ptr.offset((cs / 8) as isize);
        SERIAL_PORT.write_str("CS GDT Entry: 0x");
        SERIAL_PORT.write_hex((cs_entry >> 32) as u32); // High 32 bits
        SERIAL_PORT.write_hex(cs_entry as u32);         // Low 32 bits
        SERIAL_PORT.write_str("\n");
        
        // Check DS register
        let ds: u16;
        asm!("mov {0:x}, ds", out(reg) ds, options(nomem, nostack, preserves_flags));
        SERIAL_PORT.write_str("DS: 0x");
        SERIAL_PORT.write_hex(ds as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("===================\n");
    }
}

// Safer IDT entry verification (if you want to try it later)
fn verify_idt_entries_safe() {
    unsafe {
        // Just check that IDT is loaded, don't try to read memory directly
        let mut idtr: [u8; 6] = [0; 6];
        asm!("sidt [{}]", in(reg) &mut idtr);
        
        let idt_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
        let idt_base = u32::from_le_bytes([idtr[2], idtr[3], idtr[4], idtr[5]]);
        
        SERIAL_PORT.write_str("  IDT Base: 0x");
        SERIAL_PORT.write_hex(idt_base);
        SERIAL_PORT.write_str(", Limit: 0x");
        SERIAL_PORT.write_hex(idt_limit.into());
        SERIAL_PORT.write_str("\n");
        
        // Verify it's been loaded (non-zero base and correct limit for 256 entries)
        if idt_base != 0 && idt_limit == 0x7FF {
            SERIAL_PORT.write_str("  IDT appears to be loaded correctly\n");
        } else {
            SERIAL_PORT.write_str("  WARNING: IDT may not be loaded correctly!\n");
        }
    }
}