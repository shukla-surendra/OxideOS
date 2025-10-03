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
use gui::mouse::{MouseButton};
use gui::{ colors, widgets, fonts };
use kernel::serial::SERIAL_PORT;
use kernel::{idt, interrupts, timer, pic};
use gui::window_manager::WindowManager;
use core::ptr;

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
    unsafe{
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

    // TODO FIX ALLOCATION still not working failing with test_minimal_allocation below
    unsafe {
    // Initialize the heap allocator
    crate::kernel::allocator::init_heap(&MEMORY_MAP_REQUEST);
    SERIAL_PORT.write_str("✓ Heap allocator initialized\n");
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
            unsafe{
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
            unsafe{
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
// GRAPHICS AND GUI FUNCTIONS (Fixed)
// ============================================================================
unsafe fn create_boot_screen(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();

    SERIAL_PORT.write_str("Creating boot screen...\n");

    // Professional dark background
    graphics.clear_screen(colors::dark_theme::BACKGROUND);

    // Modern taskbar at top
    graphics.fill_rect(0, 0, width, 40, colors::dark_theme::SURFACE_VARIANT);
    graphics.draw_rect(0, 40, width, 1, colors::dark_theme::BORDER, 1);

    // OS name in taskbar
    gui::fonts::draw_string(graphics, 15, 16, "OxideOS", colors::dark_theme::ACCENT_PRIMARY);

    // Initialize windows through window manager
    init_demo_windows(width, height);

    SERIAL_PORT.write_str("Boot screen created\n");
}

// unsafe fn init_demo_windows(screen_width: u64, _screen_height: u64) {
//     // Terminal window
//     let win1 = widgets::Window::new(100, 100, 400, 250, "Terminal");
//     WINDOW_MANAGER.add_window(win1);

//     // System Info window
//     let win2 = widgets::Window::new(screen_width - 320, 100, 300, 220, "System Info");
//     WINDOW_MANAGER.add_window(win2);

//     SERIAL_PORT.write_str("Demo windows initialized\n");
// }

unsafe fn draw_demo_windows(graphics: &Graphics) {
    let (width, _height) = graphics.get_dimensions();

    // Terminal window
    let win1 = widgets::Window::new(100, 100, 400, 250, "Terminal");
    win1.draw(graphics);
    
    // Terminal content area (darker)
    graphics.fill_rect(110, 140, 380, 200, colors::dark_theme::BACKGROUND);

    // System Info window
    let win2 = widgets::Window::new(width - 320, 100, 300, 220, "System Info");
    win2.draw(graphics);

    // Buttons with proper spacing
    let btn1 = widgets::Button::new(width - 290, 150, 120, 35, "Shutdown");
    btn1.draw(graphics);

    let btn2 = widgets::Button::new(width - 290, 195, 120, 35, "Restart");
    btn2.draw(graphics);

    let btn3 = widgets::Button::new(width - 290, 240, 120, 35, "Settings");
    btn3.draw(graphics);
}

// unsafe fn run_gui_with_mouse(graphics: &Graphics) {
//     let (width, height) = graphics.get_dimensions();
//     unsafe { SERIAL_PORT.write_str("Starting GUI demo with mouse support...\n") };

//     let mut frame_count = 0u64;
//     let mut last_cursor_pos = (-1i64, -1i64);
//     let mut saved_pixels = [[0u32; 11]; 19];
//     let mut last_mouse_count = 0u64;

//     loop {
//         frame_count += 1;

//         // Check mouse interrupt count every 100 frames (roughly every second)
//         if frame_count % 100 == 0 {
//             let current_count = kernel::interrupts::get_mouse_interrupt_count();
//             if current_count > last_mouse_count {
//                 SERIAL_PORT.write_str("MOUSE: ");
//                 SERIAL_PORT.write_decimal((current_count - last_mouse_count) as u32);
//                 SERIAL_PORT.write_str(" interrupts in last second\n");
//                 last_mouse_count = current_count;
//             } else if frame_count % 500 == 0 { // Every 5 seconds, report if no interrupts
//                 SERIAL_PORT.write_str("MOUSE: No interrupts detected (move mouse to test)\n");

//                 // Try polling for mouse data manually
//                 use crate::kernel::interrupts::{MOUSE_CONTROLLER};
//                 let controller_ptr = core::ptr::addr_of!(MOUSE_CONTROLLER);
//                 if let Some(ref mouse) = (*controller_ptr).as_ref() {
//                     if mouse.poll_for_data() {
//                         SERIAL_PORT.write_str("POLL: Found data but no interrupt!\n");
//                     }
//                 }
//             }
//         }

//         // Check for mouse movement and redraw cursor if needed
//         if let Some(cursor_pos) = gui::mouse:: get_mouse_position() {
//             if cursor_pos != last_cursor_pos {
//                 // Restore old position
//                 if last_cursor_pos.0 >= 0 && last_cursor_pos.1 >= 0 {
//                     graphics.restore_cursor_area(last_cursor_pos.0, last_cursor_pos.1, &saved_pixels);
//                 }
//                 // Save new position
//                 saved_pixels = graphics.save_cursor_area(cursor_pos.0, cursor_pos.1);
                
//                 // Draw cursor
//                 graphics.draw_cursor(cursor_pos.0, cursor_pos.1, 0xFFFFFFFF);
                
//                 last_cursor_pos = cursor_pos;
//             }

//             // Handle mouse clicks
//             if gui::mouse::is_mouse_button_pressed(MouseButton::Left) {
//                 unsafe{
//                     SERIAL_PORT.write_str("CLICK: Left button at (");
//                     // SERIAL_PORT.write_decimal(cursor_pos.0 as u32);
//                     // SERIAL_PORT.write_str(",");
//                     // SERIAL_PORT.write_decimal(cursor_pos.1 as u32);
//                     // SERIAL_PORT.write_str(")\n");
//                 }

//                 // Draw a small circle where clicked
//                 graphics.draw_circle(cursor_pos.0, cursor_pos.1, 5, 0xFFFF0000);
//             }
            
//             // Check RIGHT button
//             if gui::mouse::is_mouse_button_pressed(MouseButton::Right) {
//                 SERIAL_PORT.write_str("CLICK: Right button at (");
//                 SERIAL_PORT.write_decimal(cursor_pos.0 as u32);
//                 SERIAL_PORT.write_str(",");
//                 SERIAL_PORT.write_decimal(cursor_pos.1 as u32);
//                 SERIAL_PORT.write_str(")\n");

//                 // Draw a GREEN circle for right click
//                 graphics.draw_circle(cursor_pos.0, cursor_pos.1, 5, 0xFF00FF00);
//             }

//             // Check MIDDLE button
//             if gui::mouse::is_mouse_button_pressed(MouseButton::Middle) {
//                 SERIAL_PORT.write_str("CLICK: Middle button at (");
//                 SERIAL_PORT.write_decimal(cursor_pos.0 as u32);
//                 SERIAL_PORT.write_str(",");
//                 SERIAL_PORT.write_decimal(cursor_pos.1 as u32);
//                 SERIAL_PORT.write_str(")\n");

//                 // Draw a BLUE circle for middle click
//                 graphics.draw_circle(cursor_pos.0, cursor_pos.1, 5, 0xFF0000FF);
//             }
//         }

//         // Simple animation - moving progress bar
//         if frame_count % 50000 == 0 {
//             let animation_offset = ((frame_count / 50000) * 10) % (width - 200);

//             // Clear previous progress bar area
//             graphics.fill_rect(50, height - 50, width - 100, 20, colors::DARK_GRAY);

//             // Draw animated progress bar
//             graphics.fill_rect(50 + animation_offset, height - 50, 150, 20, colors::GREEN);
//             graphics.draw_rect(50, height - 50, width - 100, 20, colors::WHITE, 1);
//         }

//         // Keyboard interaction demo
//         if frame_count % 100000 == 0 {
//             // Draw a small indicator that updates periodically
//             let indicator_color = match (frame_count / 100000) % 4 {
//                 0 => colors::RED,
//                 1 => colors::GREEN,
//                 2 => colors::BLUE,
//                 _ => colors::YELLOW,
//             };

//             graphics.fill_rect(width - 30, 10, 20, 20, indicator_color);
//             graphics.draw_rect(width - 30, 10, 20, 20, colors::WHITE, 1);

//             SERIAL_PORT.write_str("GUI: Frame ");
//             SERIAL_PORT.write_decimal((frame_count / 100000) as u32);
//             SERIAL_PORT.write_str(" rendered\n");
//         }

//         core::arch::asm!("hlt");
//     }
// }
unsafe fn init_demo_windows(screen_width: u64, _screen_height: u64) {
    // Use addr_of_mut! for safe static access
    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    
    // Terminal window
    let win1 = widgets::Window::new(100, 100, 400, 250, "Terminal");
    (*wm).add_window(win1);

    // System Info window
    let win2 = widgets::Window::new(screen_width - 320, 100, 300, 220, "System Info");
    (*wm).add_window(win2);

    SERIAL_PORT.write_str("Demo windows initialized\n");
}

unsafe fn run_gui_with_mouse(graphics: &Graphics) {
    let (width, height) = graphics.get_dimensions();
    SERIAL_PORT.write_str("Starting GUI with window manager...\n");

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
            graphics.clear_screen(colors::dark_theme::BACKGROUND);
            graphics.fill_rect(0, 0, width, 40, colors::dark_theme::SURFACE_VARIANT);
            fonts::draw_string(graphics, 15, 16, "OxideOS", colors::dark_theme::ACCENT_PRIMARY);
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


    
unsafe fn test_minimal_allocation() {
    extern crate alloc;

    SERIAL_PORT.write_str("=== TESTING MINIMAL ALLOCATION ===\n");
    
    // First, check if we have ANY heap regions
    crate::kernel::allocator::debug_heap();
    
    // If we have heap, try the smallest possible allocation
    use core::alloc::{GlobalAlloc, Layout};
    
    let layout = Layout::from_size_align(8, 8).unwrap();
    let ptr = crate::kernel::allocator::ALLOCATOR.alloc(layout);
    
    if ptr.is_null() {
        SERIAL_PORT.write_str("FAILED: Could not allocate 8 bytes\n");
    } else {
        SERIAL_PORT.write_str("SUCCESS: Allocated 8 bytes at 0x");
        SERIAL_PORT.write_hex((ptr as usize >> 32) as u32);
        SERIAL_PORT.write_hex(ptr as usize as u32);
        SERIAL_PORT.write_str("\n");
        
        // Write to the memory to verify it's actually usable
        *ptr = 0x42;
        SERIAL_PORT.write_str("Memory write test passed\n");
    }
    
    SERIAL_PORT.write_str("=== MINIMAL ALLOCATION TEST COMPLETE ===\n");
}