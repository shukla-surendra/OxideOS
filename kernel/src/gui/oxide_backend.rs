//! oxide-gui-core `Backend` implementation for the OxideOS kernel.
//!
//! Bridges the existing OxideOS graphics / keyboard / mouse drivers into the
//! `Backend` trait so any oxide-gui Canvas or widget can be used directly
//! inside the kernel GUI loop.
//!
//! # Wiring
//!
//! In `kernel/Cargo.toml` add:
//! ```toml
//! oxide-gui-core = { path = "../../oxide-gui/crates/oxide-gui-core" }
//! ```
//!
//! In `gui/mod.rs` add:
//! ```
//! pub mod oxide_backend;
//! ```
//!
//! In `gui_loop.rs` / boot init, after the Graphics object is ready:
//! ```rust
//! use crate::gui::oxide_backend::OxideBackend;
//! use oxide_gui_core::Canvas;
//!
//! let mut backend = OxideBackend::new(&graphics);
//! backend.register_callbacks();           // wire keyboard/mouse into event queue
//! let mut canvas  = Canvas::new(&mut backend);
//!
//! // Draw GNOME-style widgets
//! canvas.gnome_headerbar(0, 0, 1280, "OxideOS", false, palette::SURFACE2);
//! canvas.action_row_toggle(0, 48, 480, "Night Mode", "Dark colour scheme",
//!                          true, palette::GNOME_BLUE, false, true);
//! canvas.present();
//! ```

extern crate alloc;

use oxide_gui_core::{Backend, Color, Event, Key, MouseButton};
use crate::gui::{graphics::Graphics, mouse};
use crate::kernel::drivers::keyboard::{self, ArrowKey as KbArrow};

// ── Internal event ring buffer ────────────────────────────────────────────────
// Single-threaded kernel; plain `static mut` is safe here.

const CAP: usize = 64;

#[derive(Copy, Clone)]
enum RawEv {
    Byte(u8),
    Arrow(Dir),
}

#[derive(Copy, Clone)]
enum Dir { Up, Down, Left, Right, PageUp, PageDown }

static mut BUF:  [Option<RawEv>; CAP] = [None; CAP];
static mut HEAD: usize = 0; // read cursor
static mut TAIL: usize = 0; // write cursor

unsafe fn push(ev: RawEv) {
    let next = (TAIL + 1) % CAP;
    if next != HEAD {           // drop silently if full
        BUF[TAIL] = Some(ev);
        TAIL = next;
    }
}

unsafe fn pop() -> Option<RawEv> {
    if HEAD == TAIL { return None; }
    let ev = BUF[HEAD].take();
    HEAD = (HEAD + 1) % CAP;
    ev
}

// ── Keyboard callbacks ────────────────────────────────────────────────────────
// `register_gui_key_callback` is the *secondary* slot — it fires alongside
// whatever the main GUI loop registered as the primary callback.

unsafe fn on_char(byte: u8) {
    push(RawEv::Byte(byte));
}

unsafe fn on_arrow(k: KbArrow) {
    let dir = match k {
        KbArrow::Up       => Dir::Up,
        KbArrow::Down     => Dir::Down,
        KbArrow::Left     => Dir::Left,
        KbArrow::Right    => Dir::Right,
        KbArrow::PageUp   => Dir::PageUp,
        KbArrow::PageDown => Dir::PageDown,
    };
    push(RawEv::Arrow(dir));
}

// ── OxideBackend ──────────────────────────────────────────────────────────────

/// oxide-gui `Backend` wrapping OxideOS's framebuffer, keyboard, and mouse.
///
/// The `Graphics` object uses interior mutability through raw pointers so it
/// can be borrowed as `&self` for drawing; we store it as an immutable ref and
/// present `&mut self` to the Backend trait as required.
pub struct OxideBackend<'a> {
    gfx:         &'a Graphics,
    width:       u32,
    height:      u32,
    // Previous mouse state for delta detection
    prev_mx:     i64,
    prev_my:     i64,
    prev_left:   bool,
    prev_right:  bool,
    prev_middle: bool,
    // One buffered event (allows emitting both ButtonPress and MouseMove in
    // the same poll cycle)
    pending:     Option<Event>,
}

impl<'a> OxideBackend<'a> {
    /// Create a backend wrapping the given Graphics object.
    pub fn new(gfx: &'a Graphics) -> Self {
        let (w, h) = gfx.get_dimensions();
        Self {
            gfx,
            width:       w as u32,
            height:      h as u32,
            prev_mx:     -1,
            prev_my:     -1,
            prev_left:   false,
            prev_right:  false,
            prev_middle: false,
            pending:     None,
        }
    }

    /// Register the keyboard callbacks into the driver.
    ///
    /// Uses the *secondary* (`gui_key_callback`) slot so the primary GUI-loop
    /// callback is not displaced.  Safe to call multiple times.
    pub fn register_callbacks(&self) {
        unsafe {
            keyboard::register_gui_key_callback(on_char);
            keyboard::register_arrow_key_callback(on_arrow);
        }
    }
}

impl<'a> Backend for OxideBackend<'a> {
    #[inline]
    fn width(&self)  -> u32 { self.width  }
    #[inline]
    fn height(&self) -> u32 { self.height }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        // OxideOS Graphics::fill_rect takes u64 coordinates; color format
        // is identical (0xAARRGGBB in both systems).
        self.gfx.fill_rect(x as u64, y as u64, w as u64, h as u64, color);
    }

    fn present(&mut self) {
        // Sync cached dimensions in case the kernel resized the framebuffer.
        let (w, h) = self.gfx.get_dimensions();
        self.width  = w as u32;
        self.height = h as u32;
        self.gfx.present();
    }

    fn poll_event(&mut self) -> Option<Event> {
        // 1. Drain any event buffered in a previous call.
        if let Some(ev) = self.pending.take() {
            return Some(ev);
        }

        // 2. Drain the keyboard ring buffer.
        if let Some(raw) = unsafe { pop() } {
            return Some(match raw {
                RawEv::Byte(b)  => Event::KeyDown(byte_to_key(b)),
                RawEv::Arrow(d) => Event::KeyDown(dir_to_key(d)),
            });
        }

        // 3. Poll mouse state.
        if let Some((mx, my)) = mouse::get_mouse_position() {
            let left   = mouse::is_mouse_button_pressed(mouse::MouseButton::Left);
            let right  = mouse::is_mouse_button_pressed(mouse::MouseButton::Right);
            let middle = mouse::is_mouse_button_pressed(mouse::MouseButton::Middle);

            // Button state change → emit ButtonEvent, buffer a following
            // MouseMove if the cursor also moved this tick.
            if left != self.prev_left {
                self.prev_left = left;
                if mx != self.prev_mx || my != self.prev_my {
                    self.prev_mx = mx;
                    self.prev_my = my;
                    self.pending = Some(Event::MouseMove { x: mx as i32, y: my as i32 });
                }
                return Some(Event::MouseButton {
                    x: mx as i32, y: my as i32,
                    button: MouseButton::Left, pressed: left,
                });
            }
            if right != self.prev_right {
                self.prev_right = right;
                return Some(Event::MouseButton {
                    x: mx as i32, y: my as i32,
                    button: MouseButton::Right, pressed: right,
                });
            }
            if middle != self.prev_middle {
                self.prev_middle = middle;
                return Some(Event::MouseButton {
                    x: mx as i32, y: my as i32,
                    button: MouseButton::Middle, pressed: middle,
                });
            }

            // Pure cursor movement.
            if mx != self.prev_mx || my != self.prev_my {
                self.prev_mx = mx;
                self.prev_my = my;
                return Some(Event::MouseMove { x: mx as i32, y: my as i32 });
            }
        }

        None
    }
}

// ── Key translation ───────────────────────────────────────────────────────────

fn byte_to_key(b: u8) -> Key {
    match b {
        0x08 | 0x7F => Key::Backspace,
        0x09        => Key::Tab,
        0x0A | 0x0D => Key::Enter,
        0x1B        => Key::Escape,
        0x20        => Key::Space,
        0x21..=0x7E => Key::Char(b as char),
        _           => Key::Unknown,
    }
}

fn dir_to_key(d: Dir) -> Key {
    match d {
        Dir::Up       => Key::Up,
        Dir::Down     => Key::Down,
        Dir::Left     => Key::Left,
        Dir::Right    => Key::Right,
        Dir::PageUp   => Key::PageUp,
        Dir::PageDown => Key::PageDown,
    }
}
