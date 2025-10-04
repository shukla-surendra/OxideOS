//! OxideOS 64-bit Kernel Main Entry Point
//!
//! This file contains the kernel's main entry point and initialization sequence
//! Updated for 64-bit long mode operation with Limine bootloader.

#![no_std]
#![no_main]
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
use gui::graphics::Graphics;
use gui::mouse::MouseButton;
use gui::{colors, widgets, fonts};
use kernel::serial::SERIAL_PORT;
use kernel::{idt, interrupts, timer, pic};
use gui::window_manager::WindowManager;
use core::ptr;

use limine::BaseRevision;
use limine::request::{FramebufferRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};

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

// Global window manager
static mut WINDOW_MANAGER: WindowManager = WindowManager::new();

// ============================================================================
// MAIN KERNEL ENTRY POINT
// ============================================================================

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // ========================================================================
    // STAGE 1: EARLY INITIALIZATION
    // ========================================================================

    // Initialize serial port first for debugging output
    unsafe {
        SERIAL_PORT.init();
        SERIAL_PORT.write_str("\n=== OXIDEOS 64-BIT KERNEL BOOT ===\n");
        SERIAL_PORT.write_str("Serial port initialized\n");
    }

    // Verify Limine base revision
    assert!(BASE_REVISION.is_supported());
    SERIAL_PORT.write_str("Limine base revision supported\n");

    // ========================================================================
    // STAGE 2: INTERRUPT SYSTEM SETUP
    // ========================================================================

    init_interrupt_system();
    

    unsafe {
    crate::kernel::syscall_handler::init();
    crate::kernel::syscall_handler::test_syscall(); // 
}

    // ========================================================================
    // STAGE 2.5: MEMORY ALLOCATOR INITIALIZATION
    // ========================================================================

    // CHOICE 1: Use original bump allocator (simple, no paging manipulation)
    // unsafe {
    //     crate::kernel::allocator::init_heap(&MEMORY_MAP_REQUEST);
    //     SERIAL_PORT.write_str("✓ Bump allocator initialized\n");
    // }

    // CHOICE 2: Use new paging allocator (manipulates page tables)
    unsafe {
        crate::kernel::paging_allocator::init_paging_heap(&MEMORY_MAP_REQUEST);
        SERIAL_PORT.write_str("✓ Paging allocator initialized\n");
        
        // Optional: Test the allocator
        test_paging_allocation();
    }

    // ========================================================================
    // STAGE 3: GRAPHICS INITIALIZATION
    // ========================================================================

    // Get framebuffer AFTER interrupt setup
    let framebuffer_response = FRAMEBUFFER_REQUEST.get_response();
    if let Some(fb_response) = framebuffer_response {
        if let Some(framebuffer) = fb_response.framebuffers().next() {
            unsafe {
                SERIAL_PORT.write_str("✓ Framebuffer acquired from Limine\n");
            }

            // Initialize GUI system
            let graphics = Graphics::new(framebuffer);
            let (width, height) = graphics.get_dimensions();
            unsafe {
                // INITIALIZE MOUSE SYSTEM HERE
                SERIAL_PORT.write_str("=== ABOUT TO INITIALIZE MOUSE ===\n");
                interrupts::init_mouse_system(width, height);
                SERIAL_PORT.write_str("=== MOUSE INIT COMPLETED ===\n");

                // Create a beautiful boot screen
                create_boot_screen(&graphics);

                // Start GUI demo with mouse support
                run_gui_with_mouse(&graphics);
            }
        } else {
            unsafe {
                SERIAL_PORT.write_str("✗ No framebuffer available\n");
                run_text_mode_kernel(); // This will never return
            }
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
// PAGING ALLOCATOR TEST FUNCTION
// ============================================================================

unsafe fn test_paging_allocation() {
    extern crate alloc;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    SERIAL_PORT.write_str("\n=== TESTING PAGING ALLOCATOR ===\n");

    // Test 1: Simple Box allocation
    SERIAL_PORT.write_str("Test 1: Box<u64> allocation...\n");
    let boxed_value = Box::new(0x1234567890ABCDEFu64);
    SERIAL_PORT.write_str("  Box allocated at: 0x");
    SERIAL_PORT.write_hex(((&*boxed_value as *const u64 as usize) >> 32) as u32);
    SERIAL_PORT.write_hex((&*boxed_value as *const u64 as usize) as u32);
    SERIAL_PORT.write_str("\n  Value: 0x");
    SERIAL_PORT.write_hex((*boxed_value >> 32) as u32);
    SERIAL_PORT.write_hex(*boxed_value as u32);
    SERIAL_PORT.write_str("\n");

    // Test 2: Vec allocation
    SERIAL_PORT.write_str("Test 2: Vec<u32> allocation...\n");
    let mut vec = Vec::new();
    for i in 0..10 {
        vec.push(i * 100);
    }
    SERIAL_PORT.write_str("  Vec with 10 elements created\n");
    SERIAL_PORT.write_str("  vec[5] = ");
    SERIAL_PORT.write_decimal(vec[5]);
    SERIAL_PORT.write_str("\n");

    // Test 3: Large allocation (multiple pages)
    SERIAL_PORT.write_str("Test 3: Large allocation (16KB)...\n");
    let large_vec: Vec<u8> = Vec::with_capacity(16 * 1024);
    SERIAL_PORT.write_str("  16KB allocation successful\n");
    SERIAL_PORT.write_str("  Capacity: ");
    SERIAL_PORT.write_decimal(large_vec.capacity() as u32);
    SERIAL_PORT.write_str(" bytes\n");

    // Test 4: Multiple small allocations
    SERIAL_PORT.write_str("Test 4: Multiple small allocations...\n");
    let mut boxes = Vec::new();
    for i in 0..5 {
        boxes.push(Box::new(i * 111));
    }
    SERIAL_PORT.write_str("  Created 5 boxed values\n");
    for (i, b) in boxes.iter().enumerate() {
        SERIAL_PORT.write_str("  boxes[");
        SERIAL_PORT.write_decimal(i as u32);
        SERIAL_PORT.write_str("] = ");
        SERIAL_PORT.write_decimal(**b);
        SERIAL_PORT.write_str("\n");
    }

    SERIAL_PORT.write_str("✓ All paging allocator tests passed!\n");
    SERIAL_PORT.write_str("=== PAGING ALLOCATOR TEST COMPLETE ===\n\n");
}

// ============================================================================
// GRAPHICS AND GUI FUNCTIONS
// ============================================================================
unsafe fn create_boot_screen(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();

    SERIAL_PORT.write_str("Creating boot screen...\n");

    // Professional dark background
    graphics.clear_screen(colors::dark_theme::BACKGROUND);

    // Initialize window manager with screen dimensions
    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    (*wm).set_screen_dimensions(width, height);

    // Draw taskbar
    (*wm).draw_taskbar(graphics);

    // Initialize windows
    init_demo_windows(width, height);

    SERIAL_PORT.write_str("Boot screen created\n");
}

unsafe fn run_gui_with_mouse(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();
    SERIAL_PORT.write_str("Starting GUI with enhanced window manager...\n");

    let mut last_cursor_pos = (-1i64, -1i64);
    let mut saved_pixels = [[0u32; 11]; 19];
    let mut last_left_button = false;
    let mut needs_redraw = true;

    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);

    loop {
        let cursor_pos = gui::mouse::get_mouse_position();
        let left_button = gui::mouse::is_mouse_button_pressed(gui::mouse::MouseButton::Left);

        // Restore old cursor position first
        if last_cursor_pos.0 >= 0 {
            graphics.restore_cursor_area(last_cursor_pos.0, last_cursor_pos.1, &saved_pixels);
        }

        // Handle mouse events
        if let Some((mx, my)) = cursor_pos {
            // Mouse moved
            if (mx, my) != last_cursor_pos {
                if (*wm).is_dragging() {
                    (*wm).handle_drag(mx as u64, my as u64);
                    needs_redraw = true;
                }
                last_cursor_pos = (mx, my);
            }

            // Mouse button pressed (edge detection)
            if left_button && !last_left_button {
                (*wm).handle_click(mx as u64, my as u64);
                needs_redraw = true;
            }

            // Mouse button released
            if !left_button && last_left_button {
                (*wm).release_drag();
            }

            last_left_button = left_button;
        }

        // Full redraw if needed
        if needs_redraw {
            // Clear screen
            graphics.clear_screen(colors::dark_theme::BACKGROUND);
            
            // Draw taskbar (always on top)
            (*wm).draw_taskbar(graphics);
            
            // Draw all windows
            (*wm).draw_all(graphics);
            
            needs_redraw = false;
        }

        // Save and draw cursor at new position
        if let Some((mx, my)) = cursor_pos {
            saved_pixels = graphics.save_cursor_area(mx, my);
            graphics.draw_cursor(mx, my, 0xFFFFFFFF);
        }

        core::arch::asm!("hlt");
    }
}
unsafe fn init_demo_windows(screen_width: u64, _screen_height: u64) {
    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    
    // Terminal window
    let win1 = widgets::Window::new(100, 100, 400, 250, "Terminal");
    (*wm).add_window(win1);

    // System Info window
    let win2 = widgets::Window::new(screen_width - 320, 100, 300, 220, "System Info");
    (*wm).add_window(win2);

    SERIAL_PORT.write_str("Demo windows initialized\n");
}

// ============================================================================
// FALLBACK TEXT MODE
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
// SYSTEM DIAGNOSTIC FUNCTIONS
// ============================================================================

unsafe fn check_system_tables_64bit() {
    SERIAL_PORT.write_str("\n=== 64-BIT SYSTEM TABLE CHECK ===\n");

    // Check GDT (64-bit format)
    let mut gdt_ptr: [u8; 10] = [0; 10];
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
    let mut idtr: [u8; 10] = [0; 10];
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

    if idt_base != 0 && idt_limit == 0xFFF {
        SERIAL_PORT.write_str("  ✓ 64-bit IDT appears loaded correctly\n");
    } else {
        SERIAL_PORT.write_str("  WARNING: 64-bit IDT may not be loaded correctly!\n");
    }
}

unsafe fn test_64bit_interrupts() {
    // Enable interrupts
    asm!("sti");

    // Unmask only timer interrupt for testing
    pic::unmask_irq(0);

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

        for _ in 0..100 {
            asm!("pause");
        }
    }

    SERIAL_PORT.write_str("  Enabling 64-bit keyboard interrupts...\n");
    pic::unmask_irq(1);
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