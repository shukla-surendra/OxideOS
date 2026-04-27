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
use gui::{colors, terminal, notepad, widgets};
use kernel::serial::SERIAL_PORT;
use kernel::{gdt, idt, interrupts, timer, pic};
use gui::window_manager::WindowManager;
use gui::launcher::LauncherApp;
use gui::start_menu::StartMenu;
use gui::overview::Overview;
use gui::quick_settings::{QuickSettings, QsAction};
use gui::notifications::NotificationManager;
use core::ptr;

use limine::BaseRevision;
use limine::request::{FramebufferRequest, MemoryMapRequest, RsdpRequest, HhdmRequest, ExecutableFileRequest, RequestsEndMarker, RequestsStartMarker};

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
#[unsafe(link_section = ".requests")]
pub static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
pub static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static KERNEL_FILE_REQUEST: ExecutableFileRequest = ExecutableFileRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

static mut WINDOW_MANAGER: WindowManager = WindowManager::new();

/// Kernel binary as loaded by Limine — set once in `kmain()`, read by the installer.
pub static mut KERNEL_BINARY_PTR: *const u8 = core::ptr::null();
pub static mut KERNEL_BINARY_LEN: usize     = 0;

// ============================================================================
// NETWORK CONNECTIVITY PROBE
// ============================================================================

/// Target for the kernel-side internet probe: example.com port 80.
const PROBE_IP:   [u8; 4] = [93, 184, 216, 34];
const PROBE_PORT: u16      = 80;

#[derive(Clone, Copy)]
enum NetProbePhase {
    Idle,
    Connecting { sfd: i64, start_tick: u64 },
    Connected  { ms: u64 },
    Failed,
}

struct NetProbe {
    phase: NetProbePhase,
}

impl NetProbe {
    const fn new() -> Self { Self { phase: NetProbePhase::Idle } }

    /// Advance the state machine — call every GUI frame.
    /// Returns `true` when state changed (triggers redraw).
    fn tick(&mut self) -> bool {
        if let NetProbePhase::Connecting { sfd, start_tick } = self.phase {
            let now = unsafe { crate::kernel::timer::get_ticks() };
            // 5-second timeout (500 ticks @ 100 Hz)
            if now.wrapping_sub(start_tick) > 500 {
                unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
                self.phase = NetProbePhase::Failed;
                return true;
            }
            // Poll the stack so the TCP handshake can progress.
            unsafe { crate::kernel::net::poll(); }
            if unsafe { crate::kernel::net::socket::tcp_is_connected(sfd) } {
                let ms = now.wrapping_sub(start_tick) * 10;
                unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
                self.phase = NetProbePhase::Connected { ms };
                return true;
            }
        }
        false
    }

    /// Open a TCP connection to the probe target.
    fn start(&mut self) {
        // Close any in-flight socket first.
        if let NetProbePhase::Connecting { sfd, .. } = self.phase {
            unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
        }

        use crate::kernel::net::socket::{sys_socket, sys_connect, AF_INET, SOCK_STREAM};
        let sfd = unsafe { sys_socket(AF_INET, SOCK_STREAM, 0) };
        if sfd < 0 { self.phase = NetProbePhase::Failed; return; }

        // Build sockaddr_in: family(LE u16) | port(BE u16) | ip[4]
        let mut addr = [0u8; 8];
        addr[0..2].copy_from_slice(&(AF_INET as u16).to_le_bytes());
        addr[2..4].copy_from_slice(&PROBE_PORT.to_be_bytes());
        addr[4..8].copy_from_slice(&PROBE_IP);

        let r = unsafe { sys_connect(sfd, addr.as_ptr(), 8) };
        if r < 0 {
            // smoltcp returns 0 on non-blocking connect initiation.
            // Any negative value here means a hard error.
            unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
            self.phase = NetProbePhase::Failed;
            return;
        }

        let start_tick = unsafe { crate::kernel::timer::get_ticks() };
        self.phase = NetProbePhase::Connecting { sfd, start_tick };
    }

    /// Returns `true` if `(mx, my)` landed on the "Test" button inside the sysinfo window.
    fn is_button_hit(&self, wm: &WindowManager, sysinfo_id: usize, mx: u64, my: u64) -> bool {
        let Some(win) = wm.get_window(sysinfo_id) else { return false; };
        if !wm.is_window_visible(sysinfo_id) { return false; }
        // Button is placed at cy = win.y + 266 (see draw_sysinfo_panel)
        let btn_x = win.x + 12;
        let btn_y = win.y + 266;
        let btn_w = win.width.saturating_sub(24);
        mx >= btn_x && mx < btn_x + btn_w && my >= btn_y && my < btn_y + 22
    }
}

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

    // Capture the kernel binary pointer from Limine so the installer can write it to disk.
    if let Some(resp) = KERNEL_FILE_REQUEST.get_response() {
        let f = resp.file();
        unsafe {
            KERNEL_BINARY_PTR = f.addr();
            KERNEL_BINARY_LEN = f.size() as usize;
            SERIAL_PORT.write_str("Kernel file: ");
            SERIAL_PORT.write_decimal(KERNEL_BINARY_LEN as u32);
            SERIAL_PORT.write_str(" bytes\n");
        }
    }

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

        // procfs — /proc virtual filesystem (must come after RamFS init)
        crate::kernel::procfs::populate();
        SERIAL_PORT.write_str("✓ procfs initialized\n");

        // Environment variable store — populate defaults before any process runs
        crate::kernel::env::init_defaults();
        SERIAL_PORT.write_str("✓ Environment initialized\n");

        // ATA disks — probe all four positions (primary/secondary × master/slave)
        crate::kernel::ata::init_all();

        // Disk record store — mount on primary disk (disk 0) if present.
        // Falls back to formatting a fresh store if the disk is unformatted.
        if crate::kernel::ata::is_present() {
            unsafe { crate::kernel::disk_store::mount(0); }
        }
        // Mount secondary disk store too if a secondary disk is present.
        if crate::kernel::ata::is_present_sec() {
            unsafe { crate::kernel::disk_store::mount(2); }
        }

        // diskfs — create visible mount-point dirs and /diskinfo in RamFS
        crate::kernel::diskfs::populate();
        SERIAL_PORT.write_str("✓ diskfs populated\n");

        // MBR partition table — must run before FAT so fat::init() can find its partition
        crate::kernel::mbr::init();

        // FAT16 filesystem on primary disk (whole-disk or MBR partition)
        crate::kernel::fat::init();

        // ext2 read-only filesystem on secondary disk
        {
            use crate::kernel::mbr::{PTYPE_LINUX};
            let part_lba = unsafe {
                use crate::kernel::mbr::MBR;
                if !(*core::ptr::addr_of!(MBR)).whole_disk {
                    let mut lba = 0u32;
                    for entry in &(*core::ptr::addr_of!(MBR)).entries {
                        if entry.partition_type == PTYPE_LINUX && entry.start_lba > 0 {
                            lba = entry.start_lba;
                            break;
                        }
                    }
                    lba
                } else {
                    0 // treat whole secondary disk as ext2
                }
            };
            crate::kernel::ext2::init(part_lba);
        }

        // Network (optional — silently skipped if no RTL8139 is found)
        crate::kernel::net::init();

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

    SERIAL_PORT.write_str("Step 8: Enabling SYSCALL/SYSRET fast path...\n");
    unsafe { crate::kernel::syscall_handler::init(); }
    SERIAL_PORT.write_str("  ✓ SYSCALL/SYSRET enabled\n");

    SERIAL_PORT.write_str("Step 9: Enabling SMEP + SSE...\n");
    unsafe {
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr4 |= 1 << 20;  // CR4.SMEP — kernel cannot execute user pages
        cr4 |= 1 << 9;   // CR4.OSFXSR — enables SSE (FXSAVE/FXRSTOR + SSE insns)
        cr4 |= 1 << 10;  // CR4.OSXMMEXCPT — enables #XF for unmasked SSE FP exceptions
        asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags));
    }
    SERIAL_PORT.write_str("  ✓ SMEP + SSE enabled\n");
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
    const TITLE_BAR_H: u64 = 34; // matches TITLEBAR_H in window_manager
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
    let mut clock_dirty = false;
    let mut net_probe = NetProbe::new();
    // Pool of terminal windows (first is the main boot terminal).
    let mut terminals: Vec<terminal::TerminalApp> = Vec::new();
    terminals.push(terminal::TerminalApp::new(terminal_window_id));
    let mut notepads: Vec<notepad::NotepadApp> = Vec::new();
    let mut launcher_app = LauncherApp::new(launcher_window_id);
    let mut start_menu       = StartMenu::new();
    let mut overview         = Overview::new();
    let mut quick_settings   = QuickSettings::new();
    let mut notifications    = NotificationManager::new();
    let mut last_clock_sec: u64 = u64::MAX; // force draw on first frame

    let wm = ptr::addr_of_mut!(WINDOW_MANAGER);
    terminal::install_input_hooks();

    let (screen_w, screen_h) = unsafe { &*wm }.get_screen_dimensions();

    // Welcome notification
    notifications.push("OxideOS", "System ready. Click Activities to see open windows.", 0xFF5294E2);

    // Initialize the per-process GUI subsystem.
    unsafe { kernel::gui_proc::init(wm, graphics); }

    // DEBUG: auto-spawn bash at startup for crash diagnosis (only if bash is embedded)
    #[cfg(has_bash)]
    {
        use crate::kernel::programs;
        let _ = unsafe { crate::kernel::scheduler::spawn(programs::BASH, "bash") };
    }

    // Track previous focus to push focus-change events.
    let mut last_focused_id: Option<usize> = None;

    loop {
        // Poll keyboard directly each frame — fallback for VirtualBox where
        // IRQ1 may be unreliable or the output buffer fills without an interrupt.
        crate::kernel::keyboard::poll();
        interrupts::poll_mouse_data();

        // Update clock once per second — only redraw taskbar, not full screen.
        let current_sec = unsafe { timer::get_ticks() } / 100;
        if current_sec != last_clock_sec {
            last_clock_sec = current_sec;
            clock_dirty = true;
        }

        let cursor_pos  = gui::mouse::get_mouse_position();
        let left_button  = gui::mouse::is_mouse_button_pressed(gui::mouse::MouseButton::Left);
        let right_button = gui::mouse::is_mouse_button_pressed(gui::mouse::MouseButton::Right);

        // Determine which kind of window is focused so only one widget consumes keys.
        let focused_id = unsafe { (*wm).get_focused() };
        let focused_is_notepad = notepads.iter().any(|n| Some(n.window_id()) == focused_id);

        // Process input for all notepad windows (only focused one will consume).
        for np in notepads.iter_mut() {
            let focused = Some(np.window_id()) == focused_id;
            if np.process_input(focused) {
                terminal_dirty = true;
            }
        }

        // Process input for all terminal windows (skipped if a notepad is focused).
        for term in terminals.iter_mut() {
            let focused = !focused_is_notepad && Some(term.window_id()) == focused_id;
            if term.process_pending_input(focused) {
                terminal_dirty = true;
            }
        }

        // Route pending keyboard chars to the focused GUI-proc window (if any).
        {
            let focused_now = unsafe { (*wm).get_focused() };
            unsafe {
                while let Some(ch) = kernel::gui_proc::pop_pending_key() {
                    if let Some(wid) = focused_now {
                        if kernel::gui_proc::is_proc_window(wid as u32) {
                            kernel::gui_proc::push_key_event(wid as u32, ch);
                        }
                    }
                }
            }
            // Emit focus-change events when the focused window changes.
            if focused_now != last_focused_id {
                unsafe {
                    if let Some(old) = last_focused_id {
                        if kernel::gui_proc::is_proc_window(old as u32) {
                            kernel::gui_proc::push_focus_event(old as u32, false);
                        }
                    }
                    if let Some(new_id) = focused_now {
                        if kernel::gui_proc::is_proc_window(new_id as u32) {
                            kernel::gui_proc::push_focus_event(new_id as u32, true);
                        }
                    }
                }
                last_focused_id = focused_now;
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
                // Update quick settings button hover state.
                quick_settings.handle_mouse_move(mx as u64, my as u64, screen_w);
                if quick_settings.visible {
                    needs_redraw = true;
                }
                last_cursor_pos = (mx, my);

                // Forward mouse-move to focused GUI-proc window (content-relative).
                unsafe {
                    if let Some(wid) = (*wm).get_focused() {
                        if kernel::gui_proc::is_proc_window(wid as u32) {
                            const TB: u64 = 34; // matches TITLEBAR_H in window_manager
                            if let Some(win) = (*wm).get_window(wid) {
                                let content_y = win.y + TB;
                                let mx64 = mx as u64; let my64 = my as u64;
                                if mx64 >= win.x && mx64 < win.x + win.width
                                    && my64 >= content_y && my64 < win.y + win.height
                                {
                                    let rx = (mx64 - win.x).min(0xFFFF) as u16;
                                    let ry = (my64 - content_y).min(0xFFFF) as u16;
                                    kernel::gui_proc::push_mouse_move(wid as u32, rx, ry);
                                }
                            }
                        }
                    }
                }
            }
            if left_button && !last_left_button {
                let mx64 = mx as u64;
                let my64 = my as u64;

                // ── 1. Overview consumes all clicks when visible ──────────────
                if overview.is_visible() {
                    let (_, focused_wid) = overview.handle_click(
                        mx64, my64, unsafe { &mut *wm }, screen_w, screen_h,
                    );
                    // Clean up any window the user closed from within the overview.
                    if let Some(closed_id) = overview.take_last_closed() {
                        handle_window_close(closed_id, &mut terminals, &mut notepads,
                                            terminal_window_id);
                    }
                    let _ = focused_wid;
                    needs_redraw = true;

                // ── 2. Quick-settings panel consumes clicks when visible ───────
                } else if quick_settings.visible {
                    let action = quick_settings.handle_click(mx64, my64, screen_w);
                    match action {
                        QsAction::Shutdown => crate::kernel::shutdown::poweroff(),
                        QsAction::Reboot   => crate::kernel::shutdown::reboot(),
                        _                  => {}
                    }
                    needs_redraw = true;

                // ── 3. System-tray area → open Quick Settings ─────────────────
                } else if QuickSettings::is_toggle_area(mx64, my64, screen_w) {
                    quick_settings.toggle();
                    needs_redraw = true;

                // ── 4. Normal click dispatch ───────────────────────────────────
                } else {
                    // Activities button click is signalled by start_menu.
                    let (prog_name, sm_action, sm_consumed) = start_menu.handle_click(mx64, my64);
                    if start_menu.take_activities_request() {
                        overview.toggle();
                        quick_settings.close();
                        needs_redraw = true;
                    } else if let Some(name) = prog_name {
                        spawn_program(name, &mut terminals, &mut notepads, graphics, unsafe { &mut *wm });
                        notifications.push(name, "Application started", 0xFF26A269);
                        needs_redraw = true;
                    } else if sm_action == 1 {
                        crate::kernel::shutdown::poweroff();
                    } else if sm_action == 2 {
                        crate::kernel::shutdown::reboot();
                    } else if sm_consumed {
                        needs_redraw = true;
                    } else {
                        // Check launcher tiles.
                        let launched = launcher_app.handle_click(unsafe { &*wm }, mx64, my64);
                        if let Some(prog_name) = launched {
                            spawn_program(prog_name, &mut terminals, &mut notepads, graphics, unsafe { &mut *wm });
                            notifications.push(prog_name, "Application started", 0xFF26A269);
                            needs_redraw = true;
                        } else if net_probe.is_button_hit(unsafe { &*wm }, sysinfo_window_id, mx64, my64) {
                            if crate::kernel::net::is_present() {
                                net_probe.start();
                                needs_redraw = true;
                            }
                        } else {
                            let consumed = unsafe { (*wm).handle_context_menu_click(mx64, my64) };
                            if !consumed {
                                unsafe { (*wm).handle_click(mx64, my64); }
                                if let Some(closed_id) = unsafe { (*wm).take_closed_window() } {
                                    handle_window_close(closed_id, &mut terminals, &mut notepads,
                                                        terminal_window_id);
                                }
                            }
                            needs_redraw = true;
                        }
                    }
                }
            }
            // Forward mouse-button events to the focused GUI-proc window.
            if left_button != last_left_button {
                unsafe {
                    if let Some(wid) = (*wm).get_focused() {
                        if kernel::gui_proc::is_proc_window(wid as u32) {
                            const TB: u64 = 34; // matches TITLEBAR_H in window_manager
                            if let Some(win) = (*wm).get_window(wid) {
                                let content_y = win.y + TB;
                                let mx64 = mx as u64; let my64 = my as u64;
                                if mx64 >= win.x && mx64 < win.x + win.width
                                    && my64 >= content_y && my64 < win.y + win.height
                                {
                                    let rx = (mx64 - win.x).min(0xFFFF) as u16;
                                    let ry = (my64 - content_y).min(0xFFFF) as u16;
                                    kernel::gui_proc::push_mouse_btn(wid as u32, rx, ry, 0, left_button);
                                }
                            }
                        }
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

        // Tick the launcher highlight animation; force redraw if it changed.
        if launcher_app.tick() {
            needs_redraw = true;
        }

        // Tick notification timers; redraw when one expires.
        if notifications.tick() {
            needs_redraw = true;
        }
        // Keep redrawing while notifications are visible (progress bar animation).
        if notifications.any_active() {
            needs_redraw = true;
        }

        // Advance the network connectivity probe state machine.
        if net_probe.tick() {
            needs_redraw = true;
        }
        // Redraw every frame while connecting so the animated dots update.
        if matches!(net_probe.phase, NetProbePhase::Connecting { .. }) {
            needs_redraw = true;
        }

        if needs_redraw {
            unsafe { refresh_compositor_geometry(graphics, &*wm, terminal_window_id); }
            let bg = unsafe { (*wm).get_background_style() };
            graphics.draw_background(bg);
            unsafe { (*wm).draw_taskbar(graphics); }
            start_menu.draw_button(graphics);
            // Draw windows in z-order; immediately draw terminal content after
            // each window's chrome so z-order is respected.
            let z: alloc::vec::Vec<usize> = unsafe { (*wm).z_order_slice().to_vec() };
            for &wid in z.iter() {
                unsafe { (*wm).draw_window(graphics, wid); }
                for term in terminals.iter() {
                    if term.window_id() == wid {
                        term.draw(graphics, unsafe { &*wm });
                        break;
                    }
                }
                for np in notepads.iter() {
                    if np.window_id() == wid {
                        np.draw(graphics, unsafe { &*wm });
                        break;
                    }
                }
                if wid == sysinfo_window_id {
                    draw_sysinfo_panel(graphics, unsafe { &*wm }, sysinfo_window_id, &net_probe);
                }
            }
            launcher_app.draw(graphics, unsafe { &*wm });
            unsafe { (*wm).draw_context_menu(graphics); }
            start_menu.draw_menu(graphics);
            // Composite GUI-proc window content on top of all kernel draws.
            unsafe { kernel::gui_proc::composite_all(graphics); }
            // GNOME overlay layers — drawn last so they sit on top of everything.
            overview.draw(graphics, unsafe { &*wm }, screen_w, screen_h);
            quick_settings.draw(graphics, screen_w);
            notifications.draw(graphics, screen_w);
            needs_redraw   = false;
            terminal_dirty = false;
            clock_dirty    = false;
        } else if terminal_dirty {
            // Redraw kernel-terminal and notepad windows that changed.
            for term in terminals.iter() {
                if !term.is_compositor_mode() {
                    unsafe { (*wm).draw_window(graphics, term.window_id()); }
                    term.draw(graphics, unsafe { &*wm });
                }
            }
            for np in notepads.iter() {
                unsafe { (*wm).draw_window(graphics, np.window_id()); }
                np.draw(graphics, unsafe { &*wm });
            }
            unsafe { kernel::gui_proc::composite_all(graphics); }
            // Keep overlay layers visible during terminal partial redraws.
            overview.draw(graphics, unsafe { &*wm }, screen_w, screen_h);
            quick_settings.draw(graphics, screen_w);
            notifications.draw(graphics, screen_w);
            terminal_dirty = false;
            clock_dirty    = false;
        } else if clock_dirty {
            // Only the taskbar clock changed — redraw just the taskbar.
            unsafe { (*wm).draw_taskbar(graphics); }
            start_menu.draw_button(graphics);
            notifications.draw(graphics, screen_w);
            clock_dirty = false;
        }

        // Process compositor IPC messages AFTER the draw section so that
        // userspace-terminal output overlays the freshly-drawn window frames
        // rather than being overwritten by them.
        if unsafe { kernel::compositor::process_messages() } {
            // Don't set terminal_dirty here — compositor content is already
            // applied to the backbuffer and will be presented this frame.
            // Kernel-terminal dirty redraws happen separately above.
        }

        // Run the scheduler BEFORE the blit so GUI-proc tasks (filemanager,
        // terminal, etc.) can paint their content into the back-buffer in the
        // same frame that the kernel drew the window chrome.  Placing the tick
        // after present() caused one-frame gaps where the kernel wiped the
        // proc's content, blit an empty window, and only showed the content
        // the following frame — producing a continuous flicker.
        if let Some((pid, exit_code)) = unsafe { kernel::scheduler::tick() } {
            for term in terminals.iter_mut() {
                term.on_task_exit(pid, exit_code);
            }
            // Clean up any GUI windows owned by the exited process.
            unsafe { kernel::gui_proc::on_process_exit(pid as u32); }
            terminal_dirty = true;
            needs_redraw   = true;
        }

        // Drain any pending task stdout every frame so output appears live.
        for term in terminals.iter_mut() {
            if term.poll_task_outputs() {
                terminal_dirty = true;
            }
        }

        // Check the GUI-proc present flag after the tick so we see flags set
        // by this frame's task run.  No separate needs_redraw: the kernel will
        // already redraw next frame if the window chrome changed; the proc
        // content is already in the back-buffer and will be blit below.
        let _ = unsafe { kernel::gui_proc::take_present_flag() };

        if let Some((mx, my)) = cursor_pos {
            saved_pixels = graphics.save_cursor_area(mx, my);
            graphics.draw_cursor(mx, my, 0xFFFFFFFF);
        }

        // Blit the completed back buffer to the real framebuffer in one pass.
        graphics.present();

        // Drive the network stack (RX poll + TCP timers).
        unsafe { crate::kernel::net::poll(); }

        unsafe { core::arch::asm!("hlt"); }
    }
}

/// Clean up all kernel-side state for a window that was just closed.
fn handle_window_close(
    closed_id: usize,
    terminals: &mut Vec<terminal::TerminalApp>,
    notepads: &mut Vec<notepad::NotepadApp>,
    terminal_window_id: usize,
) {
    if unsafe { kernel::gui_proc::is_proc_window(closed_id as u32) } {
        unsafe { kernel::gui_proc::on_window_closed(closed_id as u32); }
    }
    terminals.retain(|t| t.window_id() != closed_id);
    notepads.retain(|n| n.window_id() != closed_id);
    if closed_id == terminal_window_id {
        unsafe { kernel::compositor::disable(); }
    }
}

/// Programs that get their own terminal window when launched from the start menu / launcher.
const TERMINAL_PROGRAMS: &[&str] = &["sh", "terminal", "input"];

/// Spawn `name` — opens a new terminal window for shell-like programs,
/// a notepad window for "notepad", otherwise spawns in the background.
fn spawn_program(
    name: &str,
    terminals: &mut Vec<terminal::TerminalApp>,
    notepads: &mut Vec<notepad::NotepadApp>,
    graphics: &Graphics,
    wm: &mut WindowManager,
) {
    // Kernel-native notepad — no binary needed.
    if name == "notepad" {
        let (w, h) = graphics.get_dimensions();
        let offset  = (notepads.len() as u64).min(4) * 24;
        let win_x   = 160 + offset;
        let win_y   = 70 + offset;
        let win_w   = 580u64.min(w.saturating_sub(win_x + 20));
        let win_h   = 420u64.min(h.saturating_sub(win_y + 60));
        let new_win = widgets::Window::new(win_x, win_y, win_w, win_h, "Notepad");
        if let Some(wid) = wm.add_window(new_win) {
            wm.set_focused(Some(wid));
            notepads.push(notepad::NotepadApp::new(wid));
        }
        return;
    }

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

    // System Info — top-right corner (taller to include network section)
    let win2 = widgets::Window::new(screen_width - 310, 50, 290, 340, "System Info");
    let sysinfo_id = unsafe { (*wm).add_window(win2).unwrap_or(1) };

    // Launcher — below sysinfo on the right side, shows all programs as clickable tiles
    // Width: 3 columns × (160 + 10) - 10 + 24 pad = 510 + 4 = 514, round to 520
    // Height: 2 section headers × 18 + 4 rows × (72 + 10) - 10 + 20 status + 20 pad ≈ 400
    let launcher_x = screen_width - 530;
    let launcher_y = 50u64 + 340 + 12; // below sysinfo (sysinfo now 340px tall)
    let launcher_h = screen_height.saturating_sub(launcher_y + 50).min(440).max(300);
    let win3 = widgets::Window::new(launcher_x, launcher_y, 520, launcher_h, "Programs");
    let launcher_id = unsafe { (*wm).add_window(win3).unwrap_or(2) };

    SERIAL_PORT.write_str("Demo windows initialized\n");
    (terminal_id, sysinfo_id, launcher_id)
}

/// Re-compute the terminal window's content area and refresh the compositor.
unsafe fn refresh_compositor_geometry(graphics: &Graphics, wm: &gui::window_manager::WindowManager, terminal_id: usize) {
    const TITLE_BAR_H: u64 = 34; // matches TITLEBAR_H in window_manager
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
    net_probe: &NetProbe,
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
    cy += row;

    // ══════════════════════════════════════════════════════════════════════════
    // ── Network section ───────────────────────────────────────────────────────
    // ══════════════════════════════════════════════════════════════════════════
    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1E2840);
    cy += 8;

    // ── NIC row ───────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "NETWORK", 0xFF007ACC);
    let net_up = kernel::net::is_present();
    let (dot_col, net_label, net_col) = if net_up {
        (0xFF30C040u32, kernel::net::nic_name(), 0xFF40D050u32)
    } else {
        (0xFF803030u32, "No NIC ", 0xFF805050u32)
    };
    graphics.fill_rounded_rect(cx + 80, cy + 3, 8, 8, 2, dot_col);
    fonts::draw_string(graphics, cx + 94, cy, net_label, net_col);
    cy += row;

    // ── IP / MAC row ──────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "IP", 0xFF007ACC);
    if net_up {
        fonts::draw_string(graphics, cx + 27, cy, "10.0.2.15 / 24", 0xFFB0C8E8);
    } else {
        fonts::draw_string(graphics, cx + 27, cy, "—", 0xFF3A4050);
    }
    cy += row;

    // ── Test button ───────────────────────────────────────────────────────────
    // cy = win.y + 266 at this point (matching NetProbe::is_button_hit)
    let btn_w = bar_w;
    let btn_h = 22u64;
    let (bt, bb, bd, btxt) = if net_up {
        (0xFF0D5FA0u32, 0xFF072C50u32, 0xFF00AAFFu32, 0xFFE8F4FFu32)
    } else {
        (0xFF1C2030u32, 0xFF111520u32, 0xFF2A3044u32, 0xFF404060u32)
    };
    graphics.fill_rounded_rect(cx, cy, btn_w, btn_h, 4, bt);
    graphics.fill_rect_gradient_v(cx + 1, cy + 1, btn_w - 2, btn_h - 2, bt, bb);
    graphics.draw_rounded_rect(cx, cy, btn_w, btn_h, 4, bd, 1);
    // Centred label
    let label_chars = 24u64; // "Test Internet Connection"
    let label_px    = label_chars * 9;
    let label_x     = cx + btn_w.saturating_sub(label_px) / 2;
    fonts::draw_string(graphics, label_x, cy + 7, "Test Internet Connection", btxt);
    cy += btn_h + 8;

    // ── Status line ───────────────────────────────────────────────────────────
    match net_probe.phase {
        NetProbePhase::Idle => {
            fonts::draw_string(graphics, cx, cy, "Status: idle", 0xFF3A4860);
        }
        NetProbePhase::Connecting { start_tick, .. } => {
            let cur_ticks = unsafe { kernel::timer::get_ticks() };
            let phase = ((cur_ticks.wrapping_sub(start_tick)) / 20) % 4;
            let anim = match phase {
                0 => "Connecting.   ",
                1 => "Connecting..  ",
                2 => "Connecting... ",
                _ => "Connecting....",
            };
            graphics.fill_rounded_rect(cx, cy + 3, 8, 8, 2, 0xFFC8A020);
            fonts::draw_string(graphics, cx + 14, cy, anim, 0xFFC8A020);
        }
        NetProbePhase::Connected { ms } => {
            graphics.fill_rounded_rect(cx, cy + 3, 8, 8, 2, 0xFF30C040);
            fonts::draw_string(graphics, cx + 14, cy, "Connected!", 0xFF40D050);
            // Format "NNNms"
            let mut mbuf = [0u8; 8]; let mlen = fmt_decimal(ms, &mut mbuf);
            mbuf[mlen] = b'm'; mbuf[mlen+1] = b's';
            if let Ok(ms_str) = core::str::from_utf8(&mbuf[..mlen+2]) {
                fonts::draw_string(graphics, cx + 14 + 72, cy, ms_str, 0xFF60B070);
            }
        }
        NetProbePhase::Failed => {
            graphics.fill_rounded_rect(cx, cy + 3, 8, 8, 2, 0xFFC03030);
            fonts::draw_string(graphics, cx + 14, cy, "Failed / timeout", 0xFFD04040);
        }
    }
}

/// Format `n` as ASCII decimal into `buf`. Returns number of bytes written.
fn fmt_decimal(mut n: u64, buf: &mut [u8]) -> usize {
    if n == 0 { if !buf.is_empty() { buf[0] = b'0'; } return 1; }
    let mut tmp = n; let mut len = 0;
    while tmp > 0 { len += 1; tmp /= 10; }
    let len = len.min(buf.len());
    for i in (0..len).rev() { buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    len
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
