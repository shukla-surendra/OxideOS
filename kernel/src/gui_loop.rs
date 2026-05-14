//! Main GUI event loop and all of its immediate helpers.
//!
//! Entry point: `run_gui_with_mouse` — called once from `kmain` after the
//! framebuffer and boot screen are ready.  Runs forever (halts between frames).
//!
//! Helper functions:
//!   create_boot_screen       — set up the initial windows and compositor
//!   init_demo_windows        — place terminal + sysinfo windows on screen
//!   spawn_program            — open a new terminal/notepad or background-spawn
//!   handle_window_close      — clean up kernel state for a closed window
//!   refresh_compositor_geometry — sync compositor clip rect to window position

extern crate alloc;
use alloc::vec::Vec;
use core::ptr;

use crate::gui::graphics::{Graphics, BackgroundStyle};
use crate::gui::window_manager::WindowManager;
use crate::gui::{colors, terminal, notepad, widgets, mouse};
use crate::gui::launcher::LauncherApp;
use crate::gui::start_menu::StartMenu;
use crate::gui::overview::Overview;
use crate::gui::quick_settings::{QuickSettings, QsAction};
use crate::gui::notifications::NotificationManager;
use crate::gui::calendar::CalendarPanel;
use crate::gui::menu::MenuAction;

use crate::kernel::{compositor, gui_proc, scheduler, timer, keyboard, programs, shutdown, interrupts};
use crate::net_probe::{NetProbe, NetProbePhase};
use crate::sysinfo::draw_sysinfo_panel;

/// Programs that get their own terminal window when launched.
const TERMINAL_PROGRAMS: &[&str] = &["sh", "terminal", "input"];

// ── Boot screen setup ─────────────────────────────────────────────────────────

pub unsafe fn create_boot_screen(graphics: &Graphics) -> (usize, usize) {
    let (width, height) = graphics.get_dimensions();
    crate::kernel::serial::SERIAL_PORT.write_str("Creating boot screen...\n");

    graphics.draw_background(BackgroundStyle::Default);

    let wm = ptr::addr_of_mut!(crate::WINDOW_MANAGER);
    unsafe { (*wm).set_screen_dimensions(width, height); }
    unsafe { (*wm).draw_taskbar(graphics); }

    let (terminal_id, sysinfo_id) = unsafe { init_demo_windows(width, height) };

    const TITLE_BAR_H: u64 = 34;
    if let Some(win) = unsafe { (*wm).get_window(terminal_id) } {
        unsafe {
            compositor::init(
                graphics,
                win.x, win.y + TITLE_BAR_H, win.width,
                win.height.saturating_sub(TITLE_BAR_H),
                colors::dark_theme::SURFACE,
            );
        }
    }

    crate::kernel::serial::SERIAL_PORT.write_str("Boot screen created\n");
    (terminal_id, sysinfo_id)
}

unsafe fn init_demo_windows(screen_width: u64, screen_height: u64) -> (usize, usize) {
    let wm = ptr::addr_of_mut!(crate::WINDOW_MANAGER);
    let term_h = (screen_height.saturating_sub(80 + 50)).min(420).max(260);
    let win1 = widgets::Window::new(10, 50, 540, term_h, "Terminal");
    let terminal_id = unsafe { (*wm).add_window(win1).unwrap_or(0) };
    let win2 = widgets::Window::new(screen_width - 310, 50, 290, 340, "System Info");
    let sysinfo_id = unsafe { (*wm).add_window(win2).unwrap_or(1) };
    crate::kernel::serial::SERIAL_PORT.write_str("Demo windows initialized\n");
    (terminal_id, sysinfo_id)
}

// ── Main GUI loop ─────────────────────────────────────────────────────────────

pub unsafe fn run_gui_with_mouse(
    graphics: &Graphics,
    terminal_window_id: usize,
    sysinfo_window_id:  usize,
) {
    crate::kernel::serial::SERIAL_PORT.write_str("Starting GUI with enhanced window manager...\n");

    let mut last_cursor_pos   = (-1i64, -1i64);
    let mut saved_pixels      = [[0u32; 11]; 19];
    let mut last_left_button  = false;
    let mut last_right_button = false;
    let mut needs_redraw      = true;
    let mut terminal_dirty    = false;
    let mut clock_dirty       = false;
    let mut net_probe         = NetProbe::new();

    let mut terminals: Vec<terminal::TerminalApp> = Vec::new();
    terminals.push(terminal::TerminalApp::new(terminal_window_id));
    let mut notepads:      Vec<notepad::NotepadApp> = Vec::new();
    let mut launcher_app   = LauncherApp::new();
    let mut start_menu     = StartMenu::new();
    let mut overview       = Overview::new();
    let mut quick_settings = QuickSettings::new();
    let mut notifications  = NotificationManager::new();
    let mut calendar       = CalendarPanel::new();
    let mut last_clock_sec: u64 = u64::MAX;

    let wm = ptr::addr_of_mut!(crate::WINDOW_MANAGER);
    terminal::install_input_hooks();

    let (screen_w, screen_h) = unsafe { &*wm }.get_screen_dimensions();

    notifications.push(crate::version::NAME, "System ready. Click Activities to open apps.", 0xFF5294E2);
    unsafe { gui_proc::init(wm, graphics); }

    #[cfg(has_bash)]
    { let _ = unsafe { scheduler::spawn(programs::BASH, "bash") }; }

    let mut last_focused_id: Option<usize> = None;

    loop {
        keyboard::poll();
        interrupts::poll_mouse_data();

        // Update clock once per second.
        let current_sec = unsafe { timer::get_ticks() } / 100;
        if current_sec != last_clock_sec { last_clock_sec = current_sec; clock_dirty = true; }

        let cursor_pos   = mouse::get_mouse_position();
        let left_button  = mouse::is_mouse_button_pressed(mouse::MouseButton::Left);
        let right_button = mouse::is_mouse_button_pressed(mouse::MouseButton::Right);

        let focused_id = unsafe { (*wm).get_focused() };
        let focused_is_notepad = notepads.iter().any(|n| Some(n.window_id()) == focused_id);

        for np in notepads.iter_mut() {
            let focused = Some(np.window_id()) == focused_id;
            if np.process_input(focused) { terminal_dirty = true; }
        }
        for term in terminals.iter_mut() {
            let focused = !focused_is_notepad && Some(term.window_id()) == focused_id;
            if term.process_pending_input(focused) { terminal_dirty = true; }
        }

        // Route keys to focused GUI-proc window.
        {
            let focused_now = unsafe { (*wm).get_focused() };
            unsafe {
                while let Some(ch) = gui_proc::pop_pending_key() {
                    if let Some(wid) = focused_now {
                        if gui_proc::is_proc_window(wid as u32) {
                            gui_proc::push_key_event(wid as u32, ch);
                        }
                    }
                }
            }
            if focused_now != last_focused_id {
                unsafe {
                    if let Some(old) = last_focused_id {
                        if gui_proc::is_proc_window(old as u32) {
                            gui_proc::push_focus_event(old as u32, false);
                        }
                    }
                    if let Some(new_id) = focused_now {
                        if gui_proc::is_proc_window(new_id as u32) {
                            gui_proc::push_focus_event(new_id as u32, true);
                        }
                    }
                }
                last_focused_id = focused_now;
            }
        }

        let prev_cursor_pos = last_cursor_pos;
        if last_cursor_pos.0 >= 0 {
            graphics.restore_cursor_area(last_cursor_pos.0, last_cursor_pos.1, &saved_pixels);
        }

        if let Some((mx, my)) = cursor_pos {
            if (mx, my) != last_cursor_pos {
                if unsafe { (*wm).is_dragging() } {
                    unsafe { (*wm).handle_drag(mx as u64, my as u64); }
                    needs_redraw = true;
                }
                if start_menu.handle_mouse_move(mx as u64, my as u64) { needs_redraw = true; }
                if launcher_app.handle_mouse_move(mx as u64, my as u64, screen_h) { needs_redraw = true; }
                for np in notepads.iter_mut() {
                    if np.handle_mouse_move(mx as u64, my as u64, unsafe { &*wm }) { needs_redraw = true; }
                }
                quick_settings.handle_mouse_move(mx as u64, my as u64, screen_w, left_button);
                if quick_settings.visible { needs_redraw = true; }
                last_cursor_pos = (mx, my);

                // Forward mouse-move to focused GUI-proc window.
                unsafe {
                    if let Some(wid) = (*wm).get_focused() {
                        if gui_proc::is_proc_window(wid as u32) {
                            const TB: u64 = 34;
                            if let Some(win) = (*wm).get_window(wid) {
                                let mx64 = mx as u64; let my64 = my as u64;
                                let content_y = win.y + TB;
                                if mx64 >= win.x && mx64 < win.x + win.width
                                    && my64 >= content_y && my64 < win.y + win.height
                                {
                                    gui_proc::push_mouse_move(wid as u32,
                                        (mx64 - win.x).min(0xFFFF) as u16,
                                        (my64 - content_y).min(0xFFFF) as u16);
                                }
                            }
                        }
                    }
                }
            }

            if left_button && !last_left_button {
                let mx64 = mx as u64; let my64 = my as u64;
                if notifications.handle_click(mx64, my64, screen_w) {
                    needs_redraw = true;
                } else if calendar.visible {
                    calendar.handle_click(mx64, my64, screen_w);
                    needs_redraw = true;
                } else if overview.is_visible() {
                    let (_, _) = overview.handle_click(mx64, my64, unsafe { &mut *wm }, screen_w, screen_h);
                    if let Some(closed_id) = overview.take_last_closed() {
                        handle_window_close(closed_id, &mut terminals, &mut notepads, terminal_window_id);
                    }
                    needs_redraw = true;
                } else if quick_settings.visible {
                    match quick_settings.handle_click(mx64, my64, screen_w) {
                        QsAction::Shutdown => shutdown::poweroff(),
                        QsAction::Reboot   => shutdown::reboot(),
                        _                  => {}
                    }
                    needs_redraw = true;
                } else if QuickSettings::is_toggle_area(mx64, my64, screen_w) {
                    quick_settings.toggle(); start_menu.close(); calendar.close();
                    needs_redraw = true;
                } else {
                    let (prog_name, sm_action, sm_consumed) = start_menu.handle_click(mx64, my64);
                    if start_menu.take_activities_request() {
                        launcher_app.toggle(); overview.toggle();
                        quick_settings.close(); start_menu.close();
                        needs_redraw = true;
                    } else if let Some(name) = prog_name {
                        spawn_program(name, &mut terminals, &mut notepads, graphics, unsafe { &mut *wm });
                        notifications.push(name, "Application started", 0xFF26A269);
                        needs_redraw = true;
                    } else if sm_action == 1 { shutdown::poweroff();
                    } else if sm_action == 2 { shutdown::reboot();
                    } else if sm_consumed { needs_redraw = true;
                    } else if launcher_app.visible {
                        if let Some(prog) = launcher_app.handle_click(mx64, my64, screen_h) {
                            spawn_program(prog, &mut terminals, &mut notepads, graphics, unsafe { &mut *wm });
                            notifications.push(prog, "Application started", 0xFF26A269);
                        }
                        needs_redraw = true;
                    } else if net_probe.is_button_hit(unsafe { &*wm }, sysinfo_window_id, mx64, my64) {
                        if crate::kernel::net::is_present() { net_probe.start(); needs_redraw = true; }
                    } else {
                        // Notepad menu clicks
                        let mut np_consumed = false;
                        let mut np_exit_id: Option<usize> = None;
                        for np in notepads.iter_mut() {
                            let action = np.handle_click(mx64, my64, unsafe { &*wm });
                            if action != MenuAction::None {
                                np_consumed = true; needs_redraw = true;
                                if action == MenuAction::FileExit { np_exit_id = Some(np.window_id()); }
                                break;
                            }
                        }
                        if let Some(exit_wid) = np_exit_id {
                            unsafe { (*wm).remove_window(exit_wid); }
                            handle_window_close(exit_wid, &mut terminals, &mut notepads, terminal_window_id);
                        }
                        if !np_consumed {
                            let consumed = unsafe { (*wm).handle_context_menu_click(mx64, my64) };
                            if !consumed {
                                unsafe { (*wm).handle_click(mx64, my64); }
                                if unsafe { (*wm).take_clock_click() } {
                                    calendar.toggle(); quick_settings.close(); start_menu.close();
                                }
                                if let Some(closed) = unsafe { (*wm).take_closed_window() } {
                                    handle_window_close(closed, &mut terminals, &mut notepads, terminal_window_id);
                                }
                            }
                            needs_redraw = true;
                        }
                    }
                }
            }

            // Forward mouse-button events to focused GUI-proc window.
            if left_button != last_left_button {
                unsafe {
                    if let Some(wid) = (*wm).get_focused() {
                        if gui_proc::is_proc_window(wid as u32) {
                            const TB: u64 = 34;
                            if let Some(win) = (*wm).get_window(wid) {
                                let mx64 = mx as u64; let my64 = my as u64;
                                let cy = win.y + TB;
                                if mx64 >= win.x && mx64 < win.x + win.width && my64 >= cy && my64 < win.y + win.height {
                                    gui_proc::push_mouse_btn(wid as u32,
                                        (mx64 - win.x).min(0xFFFF) as u16,
                                        (my64 - cy).min(0xFFFF) as u16, 0, left_button);
                                }
                            }
                        }
                    }
                }
            }

            if !left_button && last_left_button { unsafe { (*wm).release_drag(); } }
            if right_button && !last_right_button {
                unsafe { (*wm).handle_right_click(mx as u64, my as u64); }
                needs_redraw = true;
            }
            last_left_button  = left_button;
            last_right_button = right_button;
        }

        if launcher_app.tick() { needs_redraw = true; }
        if notifications.tick() { needs_redraw = true; }
        if net_probe.tick() { needs_redraw = true; }
        if matches!(net_probe.phase, NetProbePhase::Connecting { .. }) { needs_redraw = true; }

        // ── Draw ──────────────────────────────────────────────────────────────
        let mut did_draw = false;

        if needs_redraw {
            unsafe { refresh_compositor_geometry(graphics, &*wm, terminal_window_id); }
            let bg = unsafe { (*wm).get_background_style() };
            graphics.draw_background(bg);
            unsafe { (*wm).draw_taskbar(graphics); }
            start_menu.draw_button(graphics);
            let z: Vec<usize> = unsafe { (*wm).z_order_slice().to_vec() };
            for &wid in z.iter() {
                unsafe { (*wm).draw_window(graphics, wid); }
                for term in terminals.iter() {
                    if term.window_id() == wid { term.draw(graphics, unsafe { &*wm }); break; }
                }
                for np in notepads.iter_mut() {
                    if np.window_id() == wid { np.draw(graphics, unsafe { &*wm }); break; }
                }
                if wid == sysinfo_window_id {
                    unsafe { draw_sysinfo_panel(graphics, &*wm, sysinfo_window_id, &net_probe); }
                }
            }
            launcher_app.draw(graphics, screen_h);
            unsafe { (*wm).draw_context_menu(graphics); }
            start_menu.draw_menu(graphics);
            unsafe { gui_proc::composite_all(graphics); }
            for np in notepads.iter() { np.draw_dropdown_overlay(graphics, unsafe { &*wm }); }
            overview.draw(graphics, unsafe { &*wm }, screen_w, screen_h);
            quick_settings.draw(graphics, screen_w);
            calendar.draw(graphics, screen_w);
            notifications.draw(graphics, screen_w);
            needs_redraw = false; terminal_dirty = false; clock_dirty = false;
            did_draw = true;
        } else if terminal_dirty {
            for term in terminals.iter() {
                if !term.is_compositor_mode() {
                    unsafe { (*wm).draw_window(graphics, term.window_id()); }
                    term.draw(graphics, unsafe { &*wm });
                }
            }
            for np in notepads.iter_mut() {
                unsafe { (*wm).draw_window(graphics, np.window_id()); }
                np.draw(graphics, unsafe { &*wm });
            }
            unsafe { gui_proc::composite_all(graphics); }
            for np in notepads.iter() { np.draw_dropdown_overlay(graphics, unsafe { &*wm }); }
            overview.draw(graphics, unsafe { &*wm }, screen_w, screen_h);
            quick_settings.draw(graphics, screen_w);
            calendar.draw(graphics, screen_w);
            notifications.draw(graphics, screen_w);
            terminal_dirty = false; clock_dirty = false; did_draw = true;
        } else if clock_dirty {
            unsafe { (*wm).draw_taskbar(graphics); }
            start_menu.draw_button(graphics);
            calendar.draw(graphics, screen_w);
            notifications.draw(graphics, screen_w);
            clock_dirty = false; did_draw = true;
        }

        if unsafe { compositor::process_messages() } { did_draw = true; }

        if let Some((pid, exit_code)) = unsafe { scheduler::tick() } {
            for term in terminals.iter_mut() { term.on_task_exit(pid, exit_code); }
            unsafe { gui_proc::on_process_exit(pid as u32); }
            terminal_dirty = true; needs_redraw = true;
        }
        for term in terminals.iter_mut() {
            if term.poll_task_outputs() { terminal_dirty = true; }
        }
        if unsafe { gui_proc::take_present_flag() } { did_draw = true; }

        // Present — full blit if drew, or cursor-only blit if only cursor moved.
        let cursor_moved = cursor_pos.map(|(mx, my)| (mx, my) != prev_cursor_pos).unwrap_or(false);
        if let Some((mx, my)) = cursor_pos {
            saved_pixels = graphics.save_cursor_area(mx, my);
            graphics.draw_cursor(mx, my, 0xFFFFFFFF);
        }
        if did_draw {
            graphics.present();
        } else if cursor_moved {
            const CW: usize = 11; const CH: usize = 19;
            if prev_cursor_pos.0 >= 0 {
                graphics.present_region(prev_cursor_pos.0, prev_cursor_pos.1, CW, CH);
            }
            if let Some((mx, my)) = cursor_pos {
                graphics.present_region(mx, my, CW, CH);
            }
        }

        unsafe { crate::kernel::net::poll(); }
        unsafe { core::arch::asm!("hlt"); }
    }
}

// ── Window lifecycle helpers ──────────────────────────────────────────────────

fn handle_window_close(
    closed_id: usize,
    terminals: &mut Vec<terminal::TerminalApp>,
    notepads:  &mut Vec<notepad::NotepadApp>,
    terminal_window_id: usize,
) {
    if unsafe { gui_proc::is_proc_window(closed_id as u32) } {
        unsafe { gui_proc::on_window_closed(closed_id as u32); }
        if let Some(pid) = gui_proc::pid_by_window(closed_id as u32) {
            unsafe { scheduler::kill(pid as u8); }
        }
    }
    terminals.retain(|t| t.window_id() != closed_id);
    notepads.retain(|n| n.window_id() != closed_id);
    if closed_id == terminal_window_id { unsafe { compositor::disable(); } }
}

fn spawn_program(
    name: &str,
    terminals: &mut Vec<terminal::TerminalApp>,
    notepads:  &mut Vec<notepad::NotepadApp>,
    graphics: &Graphics,
    wm: &mut WindowManager,
) {
    if name == "notepad" {
        let (w, h) = graphics.get_dimensions();
        let offset  = (notepads.len() as u64).min(4) * 24;
        let win_w   = 580u64.min(w.saturating_sub(160 + offset + 20));
        let win_h   = 420u64.min(h.saturating_sub(70  + offset + 60));
        if let Some(wid) = wm.add_window(widgets::Window::new(160 + offset, 70 + offset, win_w, win_h, "Notepad")) {
            wm.set_focused(Some(wid));
            notepads.push(notepad::NotepadApp::new(wid));
        }
        return;
    }

    let code = match programs::find(name) { Some(c) => c, None => return };

    if TERMINAL_PROGRAMS.contains(&name) {
        let (w, h) = graphics.get_dimensions();
        let offset  = (terminals.len() as u64).min(4) * 28;
        let win_w   = 540u64.min(w.saturating_sub(30 + offset + 10));
        let win_h   = 380u64.min(h.saturating_sub(60 + offset + 50));
        let title   = if name == "sh" { "Shell" } else { "Terminal" };
        if let Some(wid) = wm.add_window(widgets::Window::new(30 + offset, 60 + offset, win_w, win_h, title)) {
            wm.set_focused(Some(wid));
            let mut term = terminal::TerminalApp::new(wid);
            if let Ok(pid) = unsafe { scheduler::spawn(code, name) } { term.attach_foreground(pid); }
            terminals.push(term);
        }
    } else {
        let _ = unsafe { scheduler::spawn(code, name) };
    }
}

/// Sync the compositor clip rect to the terminal window's content area.
pub unsafe fn refresh_compositor_geometry(
    _graphics: &Graphics,
    wm: &WindowManager,
    terminal_id: usize,
) {
    const TITLE_BAR_H: u64 = 34;
    if let Some(win) = wm.get_window(terminal_id) {
        unsafe {
            compositor::update_geometry(
                win.x, win.y + TITLE_BAR_H, win.width,
                win.height.saturating_sub(TITLE_BAR_H),
            );
        }
    }
}
