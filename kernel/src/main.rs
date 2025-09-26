//! OxideOS 64-bit Kernel Main Entry Point
//! 
//! This file contains the kernel's main entry point and initialization sequence
//! Updated for 64-bit long mode operation with Limine bootloader.

#![no_std]
#![no_main]
#![feature(asm_const)]
#![feature(naked_functions)]
#![feature(abi_x86_interrupt)] // Enables x86-interrupt ABI

// ============================================================================
// MODULE DECLARATIONS
// ============================================================================
mod panic;              // panic handler
mod kernel;             // Core kernel subsystems
mod gui;                // GUI system

// ============================================================================
// IMPORTS
// ============================================================================
use core::arch::asm;
use gui::{Graphics, colors, widgets, font};
use kernel::serial::SERIAL_PORT;
use kernel::{idt, interrupts, timer, pic};

use limine::BaseRevision;
use limine::request::{FramebufferRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};
use limine::framebuffer::Framebuffer;

// ============================================================================
// LIMINE REQUESTS - Required for bootloader communication
// ============================================================================

/// Sets the base revision to the latest revision supported by the crate.
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

/// Define the start and end markers for Limine requests.
#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

// ============================================================================
// MAIN KERNEL ENTRY POINT
// ============================================================================

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // ========================================================================
    // STAGE 1: EARLY INITIALIZATION
    // ========================================================================
    
    // Initialize serial port first for debugging output
    SERIAL_PORT.init();
    SERIAL_PORT.write_str("\n=== OXIDEOS 64-BIT KERNEL BOOT ===\n");
    SERIAL_PORT.write_str("Serial port initialized\n");
    
    // Verify Limine base revision
    assert!(BASE_REVISION.is_supported());
    SERIAL_PORT.write_str("Limine base revision supported\n");

    // ========================================================================
    // STAGE 2: INTERRUPT SYSTEM SETUP
    // ========================================================================
    
    init_interrupt_system();

    // ========================================================================
    // STAGE 3: GRAPHICS INITIALIZATION
    // ========================================================================
    
    // CRITICAL FIX: Get framebuffer AFTER interrupt setup but BEFORE hcf()
    let framebuffer_response = FRAMEBUFFER_REQUEST.get_response();
    if let Some(fb_response) = framebuffer_response {
        if let Some(framebuffer) = fb_response.framebuffers().next() {
            SERIAL_PORT.write_str("✓ Framebuffer acquired from Limine\n");
            
            // Initialize GUI system
            let graphics = Graphics::new(framebuffer);
            
            // Create a beautiful boot screen
            create_boot_screen(&graphics);
            
            // Start the GUI demo loop - THIS WAS THE MISSING PART
            run_gui_demo(&graphics);
            
        } else {
            SERIAL_PORT.write_str("✗ No framebuffer available\n");
            run_text_mode_kernel(); // This will never return
        }
    } else {
        SERIAL_PORT.write_str("✗ Failed to get framebuffer response from Limine\n");
        run_text_mode_kernel(); // This will never return
    }

    // This should never be reached due to infinite loops above
    hcf();
}

// ============================================================================
// INTERRUPT SYSTEM INITIALIZATION (Organized)
// ============================================================================

unsafe fn init_interrupt_system() {
    SERIAL_PORT.write_str("=== 64-BIT INTERRUPT SYSTEM SETUP ===\n");
    
    // Step 1: Disable interrupts during setup
    SERIAL_PORT.write_str("Step 1: Disabling interrupts (CLI)...\n");
    asm!("cli");
    
    // Step 2: Check system state
    check_system_tables_64bit();
    
    // Step 3: Initialize 64-bit IDT
    SERIAL_PORT.write_str("Step 2: Initializing 64-bit IDT...\n");
    idt::init();
    SERIAL_PORT.write_str("  ✓ 64-bit IDT loaded\n");
    
    // Step 4: Verify 64-bit IDT entries
    SERIAL_PORT.write_str("Step 3: Verifying 64-bit IDT entries...\n");
    verify_idt_entries_64bit();
    
    // Step 5: Initialize PIC
    SERIAL_PORT.write_str("Step 4: Initializing PIC for 64-bit...\n");
    pic::init();
    SERIAL_PORT.write_str("  ✓ PIC remapped for 64-bit operation\n");
    
    // Step 6: Initialize timer
    SERIAL_PORT.write_str("Step 5: Initializing 64-bit timer...\n");
    timer::init(100); // 100 Hz
    SERIAL_PORT.write_str("  ✓ 64-bit timer initialized at 100Hz\n");
    
    // Step 7: Test interrupt system
    SERIAL_PORT.write_str("Step 6: Testing 64-bit interrupt system...\n");
    test_64bit_interrupts();
    
    SERIAL_PORT.write_str("✓ 64-bit interrupt system fully operational\n");
}

// ============================================================================
// GRAPHICS AND GUI FUNCTIONS (Fixed)
// ============================================================================

unsafe fn create_boot_screen(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();
    
    SERIAL_PORT.write_str("Creating boot screen...\n");
    SERIAL_PORT.write_str("Screen dimensions: ");
    SERIAL_PORT.write_decimal(width as u32);
    SERIAL_PORT.write_str("x");
    SERIAL_PORT.write_decimal(height as u32);
    SERIAL_PORT.write_str("\n");
    
    // Clear screen with dark blue background
    graphics.clear_screen(0xFF001133);
    
    // Draw title bar at top
    graphics.fill_rect(0, 0, width, 60, colors::BLUE);
    
    // Draw OxideOS logo area
    let logo_x = width / 2 - 150;
    let logo_y = 80;
    
    // Simple "logo" - just a stylized "OS" text area
    graphics.fill_rect(logo_x, logo_y, 300, 100, colors::WHITE);
    graphics.draw_rect(logo_x, logo_y, 300, 100, colors::BLACK, 3);
    
    // Draw some decorative elements
    for i in 0..5 {
        let y = logo_y + 200 + i * 15;
        graphics.draw_line(50, y as i64, (width - 50) as i64, y as i64, colors::CYAN);
    }
    
    // Draw some "windows" to make it look like a desktop
    draw_demo_windows(graphics);
    
    SERIAL_PORT.write_str("✓ Boot screen created\n");
}

unsafe fn draw_demo_windows(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();
    
    // Window 1 - Terminal-style window
    let win1 = widgets::Window::new(100, 150, 350, 200, "OxideOS Terminal");
    win1.draw(graphics);
    
    // Add some "terminal text" effect
    graphics.fill_rect(110, 190, 330, 150, colors::BLACK);
    
    // Window 2 - Control panel style
    let win2 = widgets::Window::new(width - 280, 120, 250, 180, "System Info");
    win2.draw(graphics);
    
    // Add some buttons to the control panel
    let btn1 = widgets::Button::new(width - 260, 160, 100, 30, "Shutdown");
    btn1.draw(graphics);
    
    let btn2 = widgets::Button::new(width - 260, 200, 100, 30, "Restart");  
    btn2.draw(graphics);
    
    let btn3 = widgets::Button::new(width - 260, 240, 100, 30, "Settings");
    btn3.draw(graphics);
    
    SERIAL_PORT.write_str("✓ Demo windows drawn\n");
}

unsafe fn run_gui_demo(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();
    
    SERIAL_PORT.write_str("Starting GUI demo loop...\n");
    
    let mut frame_count = 0u64;
    let mut animation_offset = 0;
    
    loop {
        frame_count += 1;
        
        // Simple animation - moving progress bar
        if frame_count % 50000 == 0 {
            animation_offset = (animation_offset + 10) % (width as i32 - 200);
            
            // Clear previous progress bar area
            graphics.fill_rect(50, height - 50, width - 100, 20, colors::DARK_GRAY);
            
            // Draw animated progress bar
            graphics.fill_rect(50 + animation_offset as u64, height - 50, 150, 20, colors::GREEN);
            graphics.draw_rect(50, height - 50, width - 100, 20, colors::WHITE, 1);
        }
        
        // Keyboard interaction demo
        if frame_count % 100000 == 0 {
            // Draw a small indicator that updates periodically
            let indicator_color = match (frame_count / 100000) % 4 {
                0 => colors::RED,
                1 => colors::GREEN, 
                2 => colors::BLUE,
                _ => colors::YELLOW,
            };
            
            graphics.fill_rect(width - 30, 10, 20, 20, indicator_color);
            graphics.draw_rect(width - 30, 10, 20, 20, colors::WHITE, 1);
            
            SERIAL_PORT.write_str("GUI: Frame ");
            SERIAL_PORT.write_decimal((frame_count / 100000) as u32);
            SERIAL_PORT.write_str(" rendered\n");
        }
        
        // Handle interrupts (including timer and keyboard)
        core::arch::asm!("hlt");
    }
}

// ============================================================================
// FALLBACK TEXT MODE (Commented sections preserved)
// ============================================================================

unsafe fn run_text_mode_kernel() -> ! {
    SERIAL_PORT.write_str("Running in text mode - no GUI available\n");
    
    let mut counter = 0u64;
    loop {
        counter += 1;
        if counter % 10000000 == 0 {
            SERIAL_PORT.write_str("Text mode heartbeat: ");
            SERIAL_PORT.write_decimal(counter as u32);
            SERIAL_PORT.write_str("\n");
        }
        core::arch::asm!("hlt");
    }
}

// ============================================================================
// UTILITY FUNCTIONS (Preserved from original)
// ============================================================================

/// Get current Code Segment register value (same in 64-bit)
#[inline]
fn current_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}

/// Check 64-bit stack pointer in main loop (Preserved but not currently used)
#[allow(dead_code)]
unsafe fn check_stack_in_main_loop_64bit(iteration: u64) {
    let rsp: u64;
    core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
    
    // Check stack periodically
    if iteration % 10000000 == 0 {
        SERIAL_PORT.write_str("64-bit Main loop RSP: 0x");
        SERIAL_PORT.write_hex((rsp >> 32) as u32);
        SERIAL_PORT.write_hex(rsp as u32);
        
        // Adjusted validation for 64-bit (higher addresses expected)
        let valid = if rsp == 0 {
            SERIAL_PORT.write_str(" **NULL**");
            false
        } else if rsp < 0x100000 {  // Below 1MB - too low for 64-bit
            SERIAL_PORT.write_str(" **TOO_LOW**");
            false
        } else if rsp % 16 != 0 {  // Must be 16-byte aligned in 64-bit
            SERIAL_PORT.write_str(" **MISALIGNED**");
            false
        } else {
            SERIAL_PORT.write_str(" OK");
            true
        };
        
        SERIAL_PORT.write_str("\n");
        
        if !valid {
            SERIAL_PORT.write_str("64-BIT STACK CORRUPTION DETECTED!\n");
            SERIAL_PORT.write_str("RSP: 0x");
            SERIAL_PORT.write_hex((rsp >> 32) as u32);
            SERIAL_PORT.write_hex(rsp as u32);
            SERIAL_PORT.write_str("\n");
            
            core::arch::asm!("cli");
            loop { core::arch::asm!("hlt"); }
        }
    }
}

// ============================================================================
// SYSTEM DIAGNOSTIC FUNCTIONS (Preserved)
// ============================================================================

unsafe fn check_system_tables_64bit() {
    SERIAL_PORT.write_str("\n=== 64-BIT SYSTEM TABLE CHECK ===\n");

    // Check GDT (64-bit format)
    let mut gdt_ptr: [u8; 10] = [0; 10]; // 64-bit GDT pointer is 10 bytes
    asm!("sgdt [{}]", in(reg) &mut gdt_ptr);
    let gdt_limit = u16::from_le_bytes([gdt_ptr[0], gdt_ptr[1]]);
    let gdt_base = u64::from_le_bytes([
        gdt_ptr[2], gdt_ptr[3], gdt_ptr[4], gdt_ptr[5],
        gdt_ptr[6], gdt_ptr[7], gdt_ptr[8], gdt_ptr[9]
    ]);
    
    SERIAL_PORT.write_str("64-bit GDT Base: 0x");
    SERIAL_PORT.write_hex((gdt_base >> 32) as u32);
    SERIAL_PORT.write_hex(gdt_base as u32);
    SERIAL_PORT.write_str(", Limit: 0x");
    SERIAL_PORT.write_hex(gdt_limit as u32);
    SERIAL_PORT.write_str("\n");
    
    // Check CS register
    let cs: u16;
    asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    SERIAL_PORT.write_str("64-bit CS: 0x");
    SERIAL_PORT.write_hex(cs as u32);
    SERIAL_PORT.write_str("\n");
    
    SERIAL_PORT.write_str("===================\n");
}

unsafe fn verify_idt_entries_64bit() {
    let mut idtr: [u8; 10] = [0; 10]; // 64-bit IDT pointer is 10 bytes
    asm!("sidt [{}]", in(reg) &mut idtr);
    let idt_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
    let idt_base = u64::from_le_bytes([
        idtr[2], idtr[3], idtr[4], idtr[5],
        idtr[6], idtr[7], idtr[8], idtr[9]
    ]);
    
    SERIAL_PORT.write_str("  64-bit IDT Base: 0x");
    SERIAL_PORT.write_hex((idt_base >> 32) as u32);
    SERIAL_PORT.write_hex(idt_base as u32);
    SERIAL_PORT.write_str(", Limit: 0x");
    SERIAL_PORT.write_hex(idt_limit as u32);
    SERIAL_PORT.write_str("\n");

    if idt_base != 0 && idt_limit == 0xFFF { // 256 * 16 - 1 for 64-bit
        SERIAL_PORT.write_str("  ✓ 64-bit IDT appears loaded correctly\n");
    } else {
        SERIAL_PORT.write_str("  WARNING: 64-bit IDT may not be loaded correctly!\n");
    }
}

unsafe fn test_64bit_interrupts() {
    // Enable interrupts
    asm!("sti");
    
    // Unmask only timer interrupt for testing
    pic::unmask_irq(0); // IRQ0 = Timer
    
    // Wait for timer interrupts
    let initial_ticks = timer::get_ticks();
    SERIAL_PORT.write_str("  Testing 64-bit timer interrupts...\n");
    SERIAL_PORT.write_str("  Initial ticks: ");
    SERIAL_PORT.write_decimal(initial_ticks as u32);
    SERIAL_PORT.write_str("\n");
    
    // Wait for 10 timer ticks
    let target_ticks = initial_ticks + 10;
    let mut timeout = 0u32;
    
    loop {
        let current_ticks = timer::get_ticks();
        if current_ticks >= target_ticks {
            SERIAL_PORT.write_str("  ✓ 64-bit timer interrupts working! Final ticks: ");
            SERIAL_PORT.write_decimal(current_ticks as u32);
            SERIAL_PORT.write_str("\n");
            break;
        }
        
        timeout += 1;
        if timeout > 1_000_000 {
            SERIAL_PORT.write_str("  TIMEOUT: No 64-bit timer interrupts received\n");
            break;
        }
        
        // Short delay
        for _ in 0..100 {
            asm!("pause"); // Better than nop for spin-wait in 64-bit
        }
    }
    
    // Also enable keyboard for interactive testing
    SERIAL_PORT.write_str("  Enabling 64-bit keyboard interrupts...\n");
    pic::unmask_irq(1); // IRQ1 = Keyboard
    SERIAL_PORT.write_str("  ✓ Press keys to test 64-bit keyboard interrupts\n");
}

// ============================================================================
// HALT AND CATCH FIRE
// ============================================================================

fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            asm!("wfi");
            #[cfg(target_arch = "loongarch64")]
            asm!("idle 0");
        }
    }
}

// ============================================================================
// FUTURE INTERRUPT HANDLERS (Preserved for future use)
// ============================================================================

// Enhanced interrupt handlers that can update GUI
#[allow(dead_code)]
pub mod gui_interrupt_handlers {
    use super::*;
    
    // This would be called from your interrupt handler
    pub unsafe fn handle_keyboard_for_gui(scancode: u8) {
        // Example: Change screen elements based on key presses
        match scancode {
            0x1E => { // 'A' key
                SERIAL_PORT.write_str("GUI: A key pressed - could trigger animation\n");
            },
            0x30 => { // 'B' key  
                SERIAL_PORT.write_str("GUI: B key pressed - could change colors\n");
            },
            0x2E => { // 'C' key
                SERIAL_PORT.write_str("GUI: C key pressed - could clear screen\n");
            },
            _ => {}
        }
    }
    
    // Timer interrupt can drive animations
    pub unsafe fn handle_timer_for_gui(tick_count: u64) {
        // Update animations, blink cursors, etc.
        if tick_count % 100 == 0 {
            SERIAL_PORT.write_str("GUI: Animation frame update\n");
        }
    }
}