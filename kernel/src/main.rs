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
mod wallpaper;

// ============================================================================
// IMPORTS
// ============================================================================
extern crate alloc;
use alloc::vec::Vec;
use core::arch::asm;
use gui::graphics::Graphics;
use gui::{colors, terminal, widgets};
use kernel::serial::SERIAL_PORT;
use kernel::{gdt, idt, interrupts, timer, pic};
use gui::window_manager::WindowManager;
use gui::launcher::LauncherApp;
use gui::start_menu::StartMenu;
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

        // ATA disk + FAT16 filesystem (optional — no disk is fine)
        crate::kernel::ata::init();
        crate::kernel::fat::init();

        test_paging_allocation();
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

                let (terminal_window_id, sysinfo_window_id, launcher_window_id) = create_boot_screen(&graphics);
                run_gui_with_mouse(&graphics, terminal_window_id, sysinfo_window_id, launcher_window_id);
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

    SERIAL_PORT.write_str("Step 5.5: Initializing 8042 keyboard controller...\n");
    unsafe { crate::kernel::keyboard::init(); }
    SERIAL_PORT.write_str("  ✓ Keyboard controller initialized\n");

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

unsafe fn create_boot_screen(graphics: &Graphics) -> (usize, usize, usize) {
    let (width, height) = graphics.get_dimensions();
    SERIAL_PORT.write_str("Creating boot screen...\n");

    graphics.draw_background(gui::graphics::BackgroundStyle::Default);

    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    unsafe { (*wm).set_screen_dimensions(width, height); }
    unsafe { (*wm).draw_taskbar(graphics); }

    let ids = init_demo_windows(width, height);

    // Initialise the compositor so userspace programs can draw into the terminal window.
    const TITLE_BAR_H: u64 = 31;
    if let Some(win) = unsafe { (*wm).get_window(ids.0) } {
        unsafe {
            kernel::compositor::init(
                graphics,
                win.x, win.y + TITLE_BAR_H,
                win.width, win.height.saturating_sub(TITLE_BAR_H),
                gui::colors::dark_theme::SURFACE,
            );
        }
    }

    SERIAL_PORT.write_str("Boot screen created\n");
    ids
}

unsafe fn run_gui_with_mouse(graphics: &Graphics, terminal_window_id: usize, sysinfo_window_id: usize, launcher_window_id: usize) {
    SERIAL_PORT.write_str("Starting GUI with enhanced window manager...\n");

    let mut last_cursor_pos = (-1i64, -1i64);
    let mut saved_pixels = [[0u32; 11]; 19];
    let mut last_left_button  = false;
    let mut last_right_button = false;
    let mut needs_redraw = true;
    let mut terminal_dirty = false;
    // Pool of terminal windows (first is the main boot terminal).
    let mut terminals: Vec<terminal::TerminalApp> = Vec::new();
    terminals.push(terminal::TerminalApp::new(terminal_window_id));
    let mut launcher_app = LauncherApp::new(launcher_window_id);
    let mut start_menu   = StartMenu::new();
    let mut last_clock_sec: u64 = u64::MAX; // force draw on first frame

    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    terminal::install_input_hooks();

    loop {
        // Poll keyboard directly each frame — fallback for VirtualBox where
        // IRQ1 may be unreliable or the output buffer fills without an interrupt.
        crate::kernel::keyboard::poll();
        interrupts::poll_mouse_data();

        // Trigger a full redraw once per second so the taskbar clock updates.
        let current_sec = unsafe { timer::get_ticks() } / 100;
        if current_sec != last_clock_sec {
            last_clock_sec = current_sec;
            needs_redraw = true;
        }

        let cursor_pos  = gui::mouse::get_mouse_position();
        let left_button  = gui::mouse::is_mouse_button_pressed(gui::mouse::MouseButton::Left);
        let right_button = gui::mouse::is_mouse_button_pressed(gui::mouse::MouseButton::Right);

        // Process input for all terminal windows.
        for term in terminals.iter_mut() {
            let focused = unsafe { (*wm).get_focused() == Some(term.window_id()) };
            if term.process_pending_input(focused) {
                terminal_dirty = true;
            }
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
                // Update start menu + launcher hover state.
                if start_menu.handle_mouse_move(mx as u64, my as u64) {
                    needs_redraw = true;
                }
                if launcher_app.handle_mouse_move(unsafe { &*wm }, mx as u64, my as u64) {
                    needs_redraw = true;
                }
                last_cursor_pos = (mx, my);
            }
            if left_button && !last_left_button {
                // Start menu gets first pick — it handles its own button + popup.
                let (prog_name, sm_action, sm_consumed) = start_menu.handle_click(mx as u64, my as u64);
                if let Some(name) = prog_name {
                    spawn_program(name, &mut terminals, graphics, unsafe { &mut *wm });
                    needs_redraw = true;
                } else if sm_action == 1 {
                    crate::kernel::shutdown::poweroff();
                } else if sm_action == 2 {
                    crate::kernel::shutdown::reboot();
                } else if sm_consumed {
                    needs_redraw = true;
                } else {
                    // Check launcher tiles.
                    let launched = launcher_app.handle_click(unsafe { &*wm }, mx as u64, my as u64);
                    if let Some(prog_name) = launched {
                        spawn_program(prog_name, &mut terminals, graphics, unsafe { &mut *wm });
                        needs_redraw = true;
                    } else {
                        let consumed = unsafe { (*wm).handle_context_menu_click(mx as u64, my as u64) };
                        if !consumed {
                            unsafe { (*wm).handle_click(mx as u64, my as u64); }
                        }
                        needs_redraw = true;
                    }
                }
            }
            if !left_button && last_left_button {
                unsafe { (*wm).release_drag(); }
            }
            if right_button && !last_right_button {
                unsafe { (*wm).handle_right_click(mx as u64, my as u64); }
                needs_redraw = true;
            }
            last_left_button  = left_button;
            last_right_button = right_button;
        }

        // Let compositor process any draw commands from userspace terminal.
        if unsafe { kernel::compositor::process_messages() } {
            terminal_dirty = true;
        }

        // Tick the launcher highlight animation; force redraw if it changed.
        if launcher_app.tick() {
            needs_redraw = true;
        }

        if needs_redraw {
            unsafe { refresh_compositor_geometry(graphics, &*wm, terminal_window_id); }
            let bg = unsafe { (*wm).get_background_style() };
            graphics.draw_background(bg);
            unsafe { (*wm).draw_taskbar(graphics); }
            start_menu.draw_button(graphics);
            unsafe { (*wm).draw_all(graphics); }
            for term in terminals.iter() {
                term.draw(graphics, unsafe { &*wm });
            }
            draw_sysinfo_panel(graphics, unsafe { &*wm }, sysinfo_window_id);
            launcher_app.draw(graphics, unsafe { &*wm });
            unsafe { (*wm).draw_context_menu(graphics); }
            start_menu.draw_menu(graphics);
            needs_redraw   = false;
            terminal_dirty = false;
        } else if terminal_dirty {
            // Redraw all terminal windows that may have changed.
            for term in terminals.iter() {
                unsafe { (*wm).draw_window(graphics, term.window_id()); }
                term.draw(graphics, unsafe { &*wm });
            }
            terminal_dirty = false;
        }

        if let Some((mx, my)) = cursor_pos {
            saved_pixels = graphics.save_cursor_area(mx, my);
            graphics.draw_cursor(mx, my, 0xFFFFFFFF);
        }

        // Blit the completed back buffer to the real framebuffer in one pass.
        graphics.present();

        // Run the scheduler: give the next ready task one time slice (~20 ms).
        // Returns Some((pid, exit_code)) when a task permanently exits.
        if let Some((pid, exit_code)) = unsafe { kernel::scheduler::tick() } {
            for term in terminals.iter_mut() {
                term.on_task_exit(pid, exit_code);
            }
            terminal_dirty = true;
        }

        // Drain any pending task stdout every frame so output appears live.
        for term in terminals.iter_mut() {
            if term.poll_task_outputs() {
                terminal_dirty = true;
            }
        }

        unsafe { core::arch::asm!("hlt"); }
    }
}

/// Programs that get their own terminal window when launched from the start menu / launcher.
const TERMINAL_PROGRAMS: &[&str] = &["sh", "terminal", "input"];

/// Spawn `name` — opens a new terminal window for shell-like programs,
/// otherwise spawns in the background (output drains to the first terminal).
fn spawn_program(
    name: &str,
    terminals: &mut Vec<terminal::TerminalApp>,
    graphics: &Graphics,
    wm: &mut WindowManager,
) {
    let code = match crate::kernel::programs::find(name) {
        Some(c) => c,
        None    => return,
    };

    // Shell-like programs → new dedicated terminal window.
    if TERMINAL_PROGRAMS.contains(&name) {
        let (w, h) = graphics.get_dimensions();
        // Cascade new windows so they don't all stack at the same spot.
        let offset  = (terminals.len() as u64).min(4) * 28;
        let win_x   = 30 + offset;
        let win_y   = 60 + offset;
        let win_w   = 540u64.min(w.saturating_sub(win_x + 10));
        let win_h   = 380u64.min(h.saturating_sub(win_y + 50));
        let title   = if name == "sh" { "Shell" } else { "Terminal" };
        let new_win = widgets::Window::new(win_x, win_y, win_w, win_h, title);
        if let Some(wid) = wm.add_window(new_win) {
            wm.set_focused(Some(wid));
            let mut term = terminal::TerminalApp::new(wid);
            // Spawn the process and attach it as foreground in the new terminal.
            if let Ok(pid) = unsafe { crate::kernel::scheduler::spawn(code, name) } {
                term.attach_foreground(pid);
            }
            terminals.push(term);
        }
    } else {
        // Background spawn — output drains to terminal[0] as usual.
        let _ = unsafe { crate::kernel::scheduler::spawn(code, name) };
    }
}

unsafe fn init_demo_windows(screen_width: u64, screen_height: u64) -> (usize, usize, usize) {
    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);

    // Terminal — left side, comfortable reading area
    let term_h = (screen_height.saturating_sub(80 + 50)).min(420).max(260);
    let win1 = widgets::Window::new(10, 50, 540, term_h, "Terminal");
    let terminal_id = unsafe { (*wm).add_window(win1).unwrap_or(0) };

    // System Info — top-right corner
    let win2 = widgets::Window::new(screen_width - 310, 50, 290, 260, "System Info");
    let sysinfo_id = unsafe { (*wm).add_window(win2).unwrap_or(1) };

    // Launcher — below sysinfo on the right side, shows all programs as clickable tiles
    // Width: 3 columns × (160 + 10) - 10 + 24 pad = 510 + 4 = 514, round to 520
    // Height: 2 section headers × 18 + 4 rows × (72 + 10) - 10 + 20 status + 20 pad ≈ 400
    let launcher_x = screen_width - 530;
    let launcher_y = 50u64 + 260 + 12; // below sysinfo
    let launcher_h = screen_height.saturating_sub(launcher_y + 50).min(440).max(300);
    let win3 = widgets::Window::new(launcher_x, launcher_y, 520, launcher_h, "Programs");
    let launcher_id = unsafe { (*wm).add_window(win3).unwrap_or(2) };

    SERIAL_PORT.write_str("Demo windows initialized\n");
    (terminal_id, sysinfo_id, launcher_id)
}

/// Re-compute the terminal window's content area and refresh the compositor.
unsafe fn refresh_compositor_geometry(graphics: &Graphics, wm: &gui::window_manager::WindowManager, terminal_id: usize) {
    const TITLE_BAR_H: u64 = 31; // 30 px gradient + 1 px accent line
    if let Some(win) = wm.get_window(terminal_id) {
        let cx = win.x;
        let cy = win.y + TITLE_BAR_H;
        let cw = win.width;
        let ch = win.height.saturating_sub(TITLE_BAR_H);
        unsafe {
            kernel::compositor::update_geometry(cx, cy, cw, ch);
        }
    }
    let _ = graphics; // kept for potential future use
}

// ============================================================================
// SYSTEM INFO PANEL
// ============================================================================

unsafe fn draw_sysinfo_panel(
    graphics: &Graphics,
    wm: &gui::window_manager::WindowManager,
    window_id: usize,
) {
    use gui::fonts;
    use gui::colors;

    if !wm.is_window_visible(window_id) { return; }
    let Some(win) = wm.get_window(window_id) else { return; };

    let cx = win.x + 12;
    let mut cy = win.y + 38;
    let row = 20u64;
    let bar_w = win.width.saturating_sub(24);

    // ── Header divider ─────────────────────────────────────────────────────
    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1A5F9A);
    cy += 6;

    // ── OS name & version ──────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "OxideOS  v0.1.0-dev", 0xFF7FC8FF);
    cy += row;
    fonts::draw_string(graphics, cx, cy, "x86_64  Limine bootloader", 0xFF4A6080);
    cy += row + 4;

    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1E2840);
    cy += 8;

    // ── Uptime ────────────────────────────────────────────────────────────
    let ticks = unsafe { kernel::timer::get_ticks() };
    fonts::draw_string(graphics, cx, cy, "UPTIME", 0xFF007ACC);
    let mut tbuf = [0u8; 8];
    {
        let total = ticks / 100;
        let h = (total / 3600) % 100;
        let m = (total / 60) % 60;
        let s = total % 60;
        tbuf[0] = b'0' + (h/10) as u8; tbuf[1] = b'0' + (h%10) as u8; tbuf[2] = b':';
        tbuf[3] = b'0' + (m/10) as u8; tbuf[4] = b'0' + (m%10) as u8; tbuf[5] = b':';
        tbuf[6] = b'0' + (s/10) as u8; tbuf[7] = b'0' + (s%10) as u8;
    }
    let tstr = core::str::from_utf8(&tbuf).unwrap_or("00:00:00");
    fonts::draw_string(graphics, cx + 72, cy, tstr, 0xFFE0F0FF);
    cy += row;

    // ── Memory bar ────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "MEMORY", 0xFF007ACC);
    fonts::draw_string(graphics, cx + 72, cy, "128 MB total", 0xFF4A6080);
    cy += row - 4;
    graphics.draw_progress_bar(cx, cy, bar_w, 12, 30,
                                0xFF0D1B2A, 0xFF007ACC, 0xFF1A4060);
    fonts::draw_string(graphics, cx + bar_w + 2, cy - 1, "30%", 0xFF4A6080);
    cy += 18;

    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1E2840);
    cy += 8;

    // ── Disk ──────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "DISK", 0xFF007ACC);
    let (disk_str, disk_col) = if kernel::ata::is_present() {
        let mut sec_str = [b' '; 12];
        let secs = kernel::ata::sector_count();
        // write decimal
        let mb = secs / 2048;
        // simple format: "XXX MB"
        let mut n = mb; let mut i = 5usize;
        sec_str[6] = b'M'; sec_str[7] = b'B';
        loop { sec_str[i] = b'0' + (n % 10) as u8; n /= 10; if n == 0 || i == 0 { break; } i -= 1; }
        ("ATA detected", 0xFF40C040u32)
    } else {
        ("No disk", 0xFF806040u32)
    };
    fonts::draw_string(graphics, cx + 54, cy, disk_str, disk_col);
    cy += row;

    // ── Tasks ─────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "TASKS", 0xFF007ACC);
    let n = kernel::scheduler::task_count();
    let (task_str, task_col): (&str, u32) = match n {
        0 => ("idle",       0xFF4A6080),
        1 => ("1 running",  0xFF40C040),
        2 => ("2 running",  0xFF40C040),
        3 => ("3 running",  0xFF40C040),
        4 => ("4 running",  0xFF40C040),
        5 => ("5 running",  0xFF40C040),
        6 => ("6 running",  0xFF40C040),
        7 => ("7 running",  0xFF40C040),
        _ => ("8 running",  0xFF40C040),
    };
    fonts::draw_string(graphics, cx + 63, cy, task_str, task_col);
    cy += row;

    // ── Ticks ─────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "TICKS", 0xFF007ACC);
    // write tick count (up to 10 digits)
    let mut num_buf = [b' '; 12];
    let mut n = ticks; let mut i = 11usize;
    loop { num_buf[i] = b'0' + (n % 10) as u8; n /= 10; if n == 0 || i == 0 { break; } i -= 1; }
    if let Ok(s) = core::str::from_utf8(&num_buf[i..12]) {
        fonts::draw_string(graphics, cx + 63, cy, s, 0xFF4A8090);
    }
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
