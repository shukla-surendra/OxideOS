//! Kernel-side compositor for OxideOS.
//!
//! Userspace programs (e.g. the terminal) send draw commands to IPC queue 1.
//! The compositor drains that queue every GUI frame and renders the commands
//! into the designated window's content area.
//!
//! # Message format (data field of an IpcMessage)
//!
//! All coordinates are **relative to the window content area** (top-left = 0,0).
//! Values are packed as little-endian u32s.
//!
//! | type_id | payload (data bytes)                                   |
//! |---------|--------------------------------------------------------|
//! | 1       | x u32, y u32, w u32, h u32, color u32  (fill rect)    |
//! | 2       | x u32, y u32, color u32, len u32, <text bytes>  (text) |
//! | 3       | (empty)                                  (present)      |
//! | 4       | x u32, y u32, w u32, h u32              (clear rect)    |

use crate::gui::graphics::Graphics;
use crate::gui::fonts;

pub const COMPOSITOR_QUEUE_ID: u32 = 1;

const MSG_FILL_RECT:  u32 = 1;
const MSG_DRAW_TEXT:  u32 = 2;
const MSG_PRESENT:    u32 = 3;
const MSG_CLEAR_RECT: u32 = 4;

// ── State ─────────────────────────────────────────────────────────────────────

/// Pointer to the kernel's long-lived `Graphics` object (lives in the main loop stack).
/// Valid whenever `GRAPHICS_VALID` is true.
static mut GRAPHICS_PTR: *const Graphics = core::ptr::null();
static mut GRAPHICS_VALID: bool = false;
/// Absolute screen coordinates of the content area's top-left corner.
static mut CONTENT_X: u64 = 0;
static mut CONTENT_Y: u64 = 0;
/// Size of the content area.
static mut CONTENT_W: u64 = 0;
static mut CONTENT_H: u64 = 0;
/// Background colour used by clear-rect.
static mut BG_COLOR: u32 = 0xFF0E1621;

/// Initialise (or re-initialise) the compositor.
///
/// `content_x/y/w/h` describe the window's **content area** in absolute screen
/// coordinates (i.e. below the title bar).  Call this once after the terminal
/// window is created, and again whenever the window moves or is resized.
pub unsafe fn init(
    graphics: &Graphics,
    content_x: u64, content_y: u64,
    content_w: u64, content_h: u64,
    bg_color: u32,
) {
    GRAPHICS_PTR   = graphics as *const Graphics;
    GRAPHICS_VALID = true;
    CONTENT_X = content_x;
    CONTENT_Y = content_y;
    CONTENT_W = content_w;
    CONTENT_H = content_h;
    BG_COLOR  = bg_color;
    // Ensure the queue exists.
    let _ = unsafe { super::ipc::msgq_create(COMPOSITOR_QUEUE_ID) };
}

/// Update the content-area geometry (called when the terminal window is moved
/// or resized so subsequent draw commands land in the right place).
pub unsafe fn update_geometry(
    content_x: u64, content_y: u64,
    content_w: u64, content_h: u64,
) {
    CONTENT_X = content_x;
    CONTENT_Y = content_y;
    CONTENT_W = content_w;
    CONTENT_H = content_h;
}

// ── Message processing ────────────────────────────────────────────────────────

/// Drain all pending compositor messages from queue 1 and render them.
/// Returns `true` if any messages were processed (caller may want to re-present).
pub unsafe fn process_messages() -> bool {
    if !GRAPHICS_VALID || GRAPHICS_PTR.is_null() { return false; }
    let gfx = unsafe { &*GRAPHICS_PTR };

    let mut processed = false;

    loop {
        let mut msg = super::ipc::Message::empty();
        if unsafe { super::ipc::msgrcv(COMPOSITOR_QUEUE_ID, &mut msg) } != 0 {
            break; // queue empty
        }
        processed = true;
        process_one(gfx, &msg);
    }

    processed
}

fn process_one(gfx: &Graphics, msg: &super::ipc::Message) {
    let (cx, cy, cw, ch, bg) = unsafe {
        (CONTENT_X, CONTENT_Y, CONTENT_W, CONTENT_H, BG_COLOR)
    };

    match msg.type_id {
        MSG_FILL_RECT => {
            if msg.size < 20 { return; }
            let x     = read_u32(&msg.data, 0) as u64;
            let y     = read_u32(&msg.data, 4) as u64;
            let w     = read_u32(&msg.data, 8) as u64;
            let h     = read_u32(&msg.data, 12) as u64;
            let color = read_u32(&msg.data, 16);
            // Clip to content area.
            let ax = cx + x.min(cw);
            let ay = cy + y.min(ch);
            let aw = w.min(cw.saturating_sub(x));
            let ah = h.min(ch.saturating_sub(y));
            if aw > 0 && ah > 0 {
                gfx.fill_rect(ax, ay, aw, ah, color);
            }
        }

        MSG_DRAW_TEXT => {
            if msg.size < 16 { return; }
            let x     = read_u32(&msg.data, 0) as u64;
            let y     = read_u32(&msg.data, 4) as u64;
            let color = read_u32(&msg.data, 8);
            let len   = read_u32(&msg.data, 12) as usize;
            let len   = len.min(msg.size as usize - 16)
                           .min(super::ipc::MAX_MSG_SIZE - 16);
            let text_bytes = &msg.data[16..16 + len];
            if let Ok(s) = core::str::from_utf8(text_bytes) {
                let ax = cx + x.min(cw);
                let ay = cy + y.min(ch);
                fonts::draw_string(gfx, ax, ay, s, color);
            }
        }

        MSG_PRESENT => {
            // The kernel's main loop calls graphics.present() each frame;
            // MSG_PRESENT is a hint that the client has finished composing a
            // frame.  No action needed here — the next loop iteration will
            // present automatically.
        }

        MSG_CLEAR_RECT => {
            if msg.size < 16 { return; }
            let x = read_u32(&msg.data, 0) as u64;
            let y = read_u32(&msg.data, 4) as u64;
            let w = read_u32(&msg.data, 8) as u64;
            let h = read_u32(&msg.data, 12) as u64;
            let ax = cx + x.min(cw);
            let ay = cy + y.min(ch);
            let aw = w.min(cw.saturating_sub(x));
            let ah = h.min(ch.saturating_sub(y));
            if aw > 0 && ah > 0 {
                gfx.fill_rect(ax, ay, aw, ah, bg);
            }
        }

        _ => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[inline]
fn read_u32(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() { return 0; }
    u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]])
}
