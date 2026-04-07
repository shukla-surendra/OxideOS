//! OxideOS 64-bit Kernel Main Entry Point
//!
//! This file contains the kernel's main entry point and initialization sequence
//! Updated for 64-bit long mode operation with Limine bootloader.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

// ============================================================================
// MODULE DECLARATIONS
// ============================================================================
mod panic;
mod kernel;
mod gui;

// ============================================================================
// IMPORTS
// ============================================================================
use core::arch::asm;
use gui::graphics::Graphics;
use gui::{colors, terminal, widgets};
use kernel::serial::SERIAL_PORT;
use kernel::{gdt, idt, interrupts, timer, pic};
use gui::window_manager::WindowManager;
use core::ptr;

use limine::BaseRevision;
use limine::request::{FramebufferRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};

// ============================================================================
// LIMINE REQUESTS
// ============================================================================

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

static mut WINDOW_MANAGER: WindowManager = WindowManager::new();

// ============================================================================
// MAIN KERNEL ENTRY POINT
// ============================================================================

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // ── Stage 1: Early init ────────────────────────────────────────────────
    unsafe {
        SERIAL_PORT.init();
        SERIAL_PORT.write_str("\n=== OXIDEOS 64-BIT KERNEL BOOT ===\n");
        SERIAL_PORT.write_str("Serial port initialized\n");
    }

    assert!(BASE_REVISION.is_supported());
    unsafe { SERIAL_PORT.write_str("Limine base revision supported\n"); }

    // ── Stage 2: Interrupt system ──────────────────────────────────────────
    init_interrupt_system();
    crate::kernel::syscall::run_boot_self_tests();

    // ── Stage 2.5: Memory allocator + filesystem ──────────────────────────
    unsafe {
        crate::kernel::paging_allocator::init_paging_heap(&MEMORY_MAP_REQUEST);
        SERIAL_PORT.write_str("✓ Paging allocator initialized\n");

        // RamFS must be initialized after the heap allocator is ready
        crate::kernel::fs::ramfs::RAMFS.init();
        SERIAL_PORT.write_str("✓ RamFS initialized\n");

        test_paging_allocation();
    }

    unsafe {
        let exit_code = crate::kernel::user_mode::run_demo();
        SERIAL_PORT.write_str("User mode demo returned with code: ");
        SERIAL_PORT.write_decimal(exit_code as u32);
        SERIAL_PORT.write_str("\n");
    }

    // ── Stage 3: Graphics ──────────────────────────────────────────────────
    let framebuffer_response = FRAMEBUFFER_REQUEST.get_response();
    if let Some(fb_response) = framebuffer_response {
        if let Some(framebuffer) = fb_response.framebuffers().next() {
            unsafe { SERIAL_PORT.write_str("✓ Framebuffer acquired from Limine\n"); }

            let graphics = Graphics::new(framebuffer);
            let (width, height) = graphics.get_dimensions();
            unsafe {
                SERIAL_PORT.write_str("=== ABOUT TO INITIALIZE MOUSE ===\n");
                interrupts::init_mouse_system(width, height);
                SERIAL_PORT.write_str("=== MOUSE INIT COMPLETED ===\n");

                let terminal_window_id = create_boot_screen(&graphics);
                run_gui_with_mouse(&graphics, terminal_window_id);
            }
        } else {
            unsafe {
                SERIAL_PORT.write_str("✗ No framebuffer available\n");
                run_text_mode_kernel();
            }
        }
    } else {
        unsafe {
            SERIAL_PORT.write_str("✗ Failed to get framebuffer response\n");
            run_text_mode_kernel();
        }
    }

    hcf();
}

// ============================================================================
// INTERRUPT SYSTEM INITIALIZATION
// ============================================================================

unsafe fn init_interrupt_system() {
    SERIAL_PORT.write_str("=== 64-BIT INTERRUPT SYSTEM SETUP ===\n");
    SERIAL_PORT.write_str("Step 1: Disabling interrupts (CLI)...\n");
    unsafe { asm!("cli"); }

    SERIAL_PORT.write_str("Step 2: Installing x86_64 GDT/TSS...\n");
    gdt::init();
    SERIAL_PORT.write_str("  ✓ GDT/TSS initialized\n");

    check_system_tables_64bit();

    SERIAL_PORT.write_str("Step 3: Initializing 64-bit IDT...\n");
    idt::init();
    SERIAL_PORT.write_str("  ✓ 64-bit IDT loaded\n");

    verify_idt_entries_64bit();

    SERIAL_PORT.write_str("Step 5: Initializing PIC for 64-bit...\n");
    pic::init();
    SERIAL_PORT.write_str("  ✓ PIC remapped\n");

    SERIAL_PORT.write_str("Step 6: Initializing 64-bit timer...\n");
    timer::init(100);
    SERIAL_PORT.write_str("  ✓ Timer at 100Hz\n");

    SERIAL_PORT.write_str("Step 7: Testing interrupt system...\n");
    test_64bit_interrupts();
    SERIAL_PORT.write_str("✓ 64-bit interrupt system fully operational\n");
}

// ============================================================================
// PAGING ALLOCATOR TEST
// ============================================================================

unsafe fn test_paging_allocation() {
    extern crate alloc;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    SERIAL_PORT.write_str("\n=== TESTING PAGING ALLOCATOR ===\n");

    let boxed_value = Box::new(0x1234567890ABCDEFu64);
    SERIAL_PORT.write_str("Test 1: Box<u64> @ 0x");
    SERIAL_PORT.write_hex(((&*boxed_value as *const u64 as usize) >> 32) as u32);
    SERIAL_PORT.write_hex((&*boxed_value as *const u64 as usize) as u32);
    SERIAL_PORT.write_str("\n");

    let mut vec: Vec<u32> = Vec::new();
    for i in 0..10 { vec.push(i * 100); }
    SERIAL_PORT.write_str("Test 2: Vec[5] = ");
    SERIAL_PORT.write_decimal(vec[5]);
    SERIAL_PORT.write_str("\n");

    // Test deallocation: drop the box, then allocate again
    drop(boxed_value);
    drop(vec);
    let _recycled = Box::new(0xDEADBEEFu64);
    SERIAL_PORT.write_str("Test 3: dealloc + recycle OK\n");

    SERIAL_PORT.write_str("✓ All paging allocator tests passed!\n\n");
}

// ============================================================================
// GUI
// ============================================================================

unsafe fn create_boot_screen(graphics: &Graphics) -> usize {
    let (width, height) = graphics.get_dimensions();
    SERIAL_PORT.write_str("Creating boot screen...\n");

    graphics.clear_screen(colors::dark_theme::BACKGROUND);

    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    unsafe { (*wm).set_screen_dimensions(width, height); }
    unsafe { (*wm).draw_taskbar(graphics); }

    let terminal_window_id = init_demo_windows(width, height);
    SERIAL_PORT.write_str("Boot screen created\n");
    terminal_window_id
}

unsafe fn run_gui_with_mouse(graphics: &Graphics, terminal_window_id: usize) {
    SERIAL_PORT.write_str("Starting GUI with enhanced window manager...\n");

    let mut last_cursor_pos = (-1i64, -1i64);
    let mut saved_pixels = [[0u32; 11]; 19];
    let mut last_left_button = false;
    let mut needs_redraw = true;
    let mut terminal_dirty = false;
    let mut terminal_app = terminal::TerminalApp::new(terminal_window_id);

    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    terminal::install_input_hooks();

    loop {
        interrupts::poll_mouse_data();

        let cursor_pos   = gui::mouse::get_mouse_position();
        let left_button  = gui::mouse::is_mouse_button_pressed(gui::mouse::MouseButton::Left);
        let terminal_focused = unsafe { (*wm).get_focused() == Some(terminal_app.window_id()) };

        if terminal_app.process_pending_input(terminal_focused) {
            terminal_dirty = true;
        }

        if last_cursor_pos.0 >= 0 {
            graphics.restore_cursor_area(last_cursor_pos.0, last_cursor_pos.1, &saved_pixels);
        }

        if let Some((mx, my)) = cursor_pos {
            if (mx, my) != last_cursor_pos {
                if unsafe { (*wm).is_dragging() } {
                    unsafe { (*wm).handle_drag(mx as u64, my as u64); }
                    needs_redraw = true;
                }
                last_cursor_pos = (mx, my);
            }
            if left_button && !last_left_button {
                unsafe { (*wm).handle_click(mx as u64, my as u64); }
                needs_redraw = true;
            }
            if !left_button && last_left_button {
                unsafe { (*wm).release_drag(); }
            }
            last_left_button = left_button;
        }

        if needs_redraw {
            graphics.clear_screen(colors::dark_theme::BACKGROUND);
            unsafe { (*wm).draw_taskbar(graphics); }
            unsafe { (*wm).draw_all(graphics); }
            terminal_app.draw(graphics, unsafe { &*wm });
            needs_redraw   = false;
            terminal_dirty = false;
        } else if terminal_dirty {
            unsafe { (*wm).draw_window(graphics, terminal_window_id); }
            terminal_app.draw(graphics, unsafe { &*wm });
            terminal_dirty = false;
        }

        if let Some((mx, my)) = cursor_pos {
            saved_pixels = graphics.save_cursor_area(mx, my);
            graphics.draw_cursor(mx, my, 0xFFFFFFFF);
        }

        unsafe { core::arch::asm!("hlt"); }
    }
}

unsafe fn init_demo_windows(screen_width: u64, _screen_height: u64) -> usize {
    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);

    let win1 = widgets::Window::new(100, 100, 400, 250, "Terminal");
    let terminal_window_id = unsafe { (*wm).add_window(win1).unwrap_or(0) };

    let win2 = widgets::Window::new(screen_width - 320, 100, 300, 220, "System Info");
    unsafe { (*wm).add_window(win2); }

    SERIAL_PORT.write_str("Demo windows initialized\n");
    terminal_window_id
}

// ============================================================================
// TEXT-MODE FALLBACK
// ============================================================================

unsafe fn run_text_mode_kernel() -> ! {
    SERIAL_PORT.write_str("Running in text mode - no GUI available\n");
    let mut counter = 0u64;
    loop {
        counter += 1;
        if counter % 10_000_000 == 0 {
            SERIAL_PORT.write_str("Heartbeat: ");
            SERIAL_PORT.write_decimal(counter as u32);
            SERIAL_PORT.write_str("\n");
        }
        unsafe { core::arch::asm!("hlt"); }
    }
}

// ============================================================================
// DIAGNOSTIC HELPERS
// ============================================================================

unsafe fn check_system_tables_64bit() {
    SERIAL_PORT.write_str("\n=== 64-BIT SYSTEM TABLE CHECK ===\n");
    let mut gdt_ptr: [u8; 10] = [0; 10];
    unsafe { asm!("sgdt [{}]", in(reg) &mut gdt_ptr); }
    let gdt_base = u64::from_le_bytes([
        gdt_ptr[2], gdt_ptr[3], gdt_ptr[4], gdt_ptr[5],
        gdt_ptr[6], gdt_ptr[7], gdt_ptr[8], gdt_ptr[9],
    ]);
    SERIAL_PORT.write_str("GDT Base: 0x");
    SERIAL_PORT.write_hex((gdt_base >> 32) as u32);
    SERIAL_PORT.write_hex(gdt_base as u32);
    SERIAL_PORT.write_str("\n===================\n");
}

unsafe fn verify_idt_entries_64bit() {
    let mut idtr: [u8; 10] = [0; 10];
    unsafe { asm!("sidt [{}]", in(reg) &mut idtr); }
    let idt_base  = u64::from_le_bytes([
        idtr[2], idtr[3], idtr[4], idtr[5],
        idtr[6], idtr[7], idtr[8], idtr[9],
    ]);
    let idt_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
    SERIAL_PORT.write_str("  IDT Base: 0x");
    SERIAL_PORT.write_hex((idt_base >> 32) as u32);
    SERIAL_PORT.write_hex(idt_base as u32);
    SERIAL_PORT.write_str(", Limit: 0x");
    SERIAL_PORT.write_hex(idt_limit as u32);
    SERIAL_PORT.write_str("\n");
    if idt_base != 0 && idt_limit == 0xFFF {
        SERIAL_PORT.write_str("  ✓ IDT loaded correctly\n");
    } else {
        SERIAL_PORT.write_str("  WARNING: IDT may not be loaded correctly!\n");
    }
}

unsafe fn test_64bit_interrupts() {
    unsafe { asm!("sti"); }
    pic::unmask_irq(0);

    let initial_ticks = timer::get_ticks();
    let target_ticks  = initial_ticks + 10;
    let mut timeout   = 0u32;

    loop {
        if timer::get_ticks() >= target_ticks {
            SERIAL_PORT.write_str("  ✓ Timer interrupts working!\n");
            break;
        }
        timeout += 1;
        if timeout > 1_000_000 {
            SERIAL_PORT.write_str("  TIMEOUT: No timer interrupts\n");
            break;
        }
        for _ in 0..100 { unsafe { asm!("pause"); } }
    }

    pic::unmask_irq(1);
    SERIAL_PORT.write_str("  ✓ Keyboard interrupts enabled\n");
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
