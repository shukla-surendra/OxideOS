//! Per-process GUI window management and event routing.
//!
//! # How it works
//!
//! A userspace program calls `GuiCreate` syscall → `create_window()` allocates a
//! slot, creates a real WM window, and returns a `window_id`.
//!
//! Drawing syscalls (`GuiFillRect`, `GuiDrawText`, …) translate window-relative
//! coordinates to absolute screen coordinates and paint directly onto the
//! back-buffer via the `Graphics` pointer.
//!
//! Events (key presses, mouse moves/clicks, focus/close) are pushed from the
//! main GUI loop via `push_*` helpers.  The process reads them one at a time
//! with `GuiPollEvent` → `poll_event()`.
//!
//! # Statics
//!
//! All state lives in fixed-size static arrays so no heap allocation is needed
//! inside the kernel.  `MAX_ENTRIES` windows are supported simultaneously.

use crate::gui::widgets::Window;
use crate::gui::graphics::Graphics;
use crate::gui::fonts;
use crate::kernel::shm;

const MAX_ENTRIES:      usize = 12;
const EVENT_RING_SIZE:  usize = 64;
const TITLE_BUF_LEN:    usize = 64;
const TITLE_BAR_H:      u64   = 31; // pixels (30 gradient + 1 accent)

// ── Event type tags ───────────────────────────────────────────────────────────

pub const GUI_EVENT_KEY:        u32 = 0;
pub const GUI_EVENT_MOUSE_MOVE: u32 = 1;
pub const GUI_EVENT_MOUSE_BTN:  u32 = 2;
pub const GUI_EVENT_FOCUS:      u32 = 3;
pub const GUI_EVENT_CLOSE:      u32 = 4;

// ── Raw event struct (matches oxide-rt's GuiEvent for zero-copy) ──────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GuiEventRaw {
    pub kind: u32,
    pub data: [u8; 12],
}

impl GuiEventRaw {
    const fn zero() -> Self { Self { kind: 0, data: [0; 12] } }

    pub fn key(ch: u8) -> Self {
        let mut d = [0u8; 12];
        d[0] = ch;
        Self { kind: GUI_EVENT_KEY, data: d }
    }

    pub fn mouse_move(x: u16, y: u16) -> Self {
        let mut d = [0u8; 12];
        d[0] = x as u8;  d[1] = (x >> 8) as u8;
        d[2] = y as u8;  d[3] = (y >> 8) as u8;
        Self { kind: GUI_EVENT_MOUSE_MOVE, data: d }
    }

    pub fn mouse_btn(x: u16, y: u16, button: u8, pressed: bool) -> Self {
        let mut d = [0u8; 12];
        d[0] = x as u8;  d[1] = (x >> 8) as u8;
        d[2] = y as u8;  d[3] = (y >> 8) as u8;
        d[4] = button;
        d[5] = pressed as u8;
        Self { kind: GUI_EVENT_MOUSE_BTN, data: d }
    }

    pub fn focus(gained: bool) -> Self {
        let mut d = [0u8; 12];
        d[0] = gained as u8;
        Self { kind: GUI_EVENT_FOCUS, data: d }
    }

    pub fn close() -> Self { Self { kind: GUI_EVENT_CLOSE, data: [0; 12] } }
}

// ── Per-window entry ──────────────────────────────────────────────────────────

struct GprocEntry {
    active:    bool,
    window_id: u32,   // WM window id
    pid:       u32,
    events:    [GuiEventRaw; EVENT_RING_SIZE],
    head:      usize,
    tail:      usize,
}

impl GprocEntry {
    const fn empty() -> Self {
        const Z: GuiEventRaw = GuiEventRaw::zero();
        Self {
            active:    false,
            window_id: 0,
            pid:       0,
            events:    [Z; EVENT_RING_SIZE],
            head:      0,
            tail:      0,
        }
    }

    fn push(&mut self, ev: GuiEventRaw) {
        let next = (self.tail + 1) % EVENT_RING_SIZE;
        if next == self.head { return; } // drop if full
        self.events[self.tail] = ev;
        self.tail = next;
    }

    fn pop(&mut self) -> Option<GuiEventRaw> {
        if self.head == self.tail { return None; }
        let ev = self.events[self.head];
        self.head = (self.head + 1) % EVENT_RING_SIZE;
        Some(ev)
    }
}

// ── Static globals ────────────────────────────────────────────────────────────

static mut ENTRIES: [GprocEntry; MAX_ENTRIES] = {
    const E: GprocEntry = GprocEntry::empty();
    [E; MAX_ENTRIES]
};

// Static title storage — avoids changing Window::title from &'static str
static mut TITLE_BUFS: [[u8; TITLE_BUF_LEN]; MAX_ENTRIES] = [[0; TITLE_BUF_LEN]; MAX_ENTRIES];
static mut TITLE_LENS: [usize; MAX_ENTRIES]                = [0; MAX_ENTRIES];

// Pointers set by `init()`.
static mut WM_PTR:  *mut crate::gui::window_manager::WindowManager = core::ptr::null_mut();
static mut GFX_PTR: *const Graphics                                 = core::ptr::null();
static mut GFX_VALID: bool                                           = false;

// Pending keyboard chars forwarded from the ISR into the main-loop buffer
// so the loop can route them to the focused window's event queue.
const KEY_RING_SIZE: usize = 128;
static mut KEY_RING: [u8; KEY_RING_SIZE] = [0; KEY_RING_SIZE];
static mut KEY_HEAD:  usize = 0;
static mut KEY_TAIL:  usize = 0;

// ── Initialization ────────────────────────────────────────────────────────────

/// Called once from the main GUI loop after WM and Graphics are set up.
pub unsafe fn init(
    wm:  *mut crate::gui::window_manager::WindowManager,
    gfx: *const Graphics,
) {
    WM_PTR    = wm;
    GFX_PTR   = gfx;
    GFX_VALID = true;
    // Register the GUI key callback with the keyboard driver.
    crate::kernel::keyboard::register_gui_key_callback(gui_key_isr_callback);
}

/// Called from the keyboard ISR to buffer a char for GUI routing.
unsafe fn gui_key_isr_callback(ch: u8) {
    unsafe {
        let next = (KEY_TAIL + 1) % KEY_RING_SIZE;
        if next != KEY_HEAD {
            KEY_RING[KEY_TAIL] = ch;
            KEY_TAIL = next;
        }
    }
}

/// Drain one pending key from the ISR ring.  Returns None if empty.
pub unsafe fn pop_pending_key() -> Option<u8> {
    unsafe {
        if KEY_HEAD == KEY_TAIL { return None; }
        let ch = KEY_RING[KEY_HEAD];
        KEY_HEAD = (KEY_HEAD + 1) % KEY_RING_SIZE;
        Some(ch)
    }
}

// ── Helper: find slot by window_id ────────────────────────────────────────────

fn slot_by_win(window_id: u32) -> Option<usize> {
    unsafe {
        for i in 0..MAX_ENTRIES {
            if ENTRIES[i].active && ENTRIES[i].window_id == window_id {
                return Some(i);
            }
        }
    }
    None
}

fn slot_by_pid_and_win(pid: u32, window_id: u32) -> Option<usize> {
    unsafe {
        for i in 0..MAX_ENTRIES {
            if ENTRIES[i].active && ENTRIES[i].pid == pid && ENTRIES[i].window_id == window_id {
                return Some(i);
            }
        }
    }
    None
}

// ── Public query ──────────────────────────────────────────────────────────────

/// True if `window_id` is owned by a GUI-proc (not a kernel terminal window).
pub fn is_proc_window(window_id: u32) -> bool {
    slot_by_win(window_id).is_some()
}

// ── Static title helper ───────────────────────────────────────────────────────

/// Return a `&'static str` backed by the entry's static title buffer.
///
/// # Safety
/// The returned reference is valid for the lifetime of the static `TITLE_BUFS`.
unsafe fn static_title(slot: usize) -> &'static str {
    unsafe {
        let len = TITLE_LENS[slot];
        let ptr = TITLE_BUFS[slot].as_ptr();
        let bytes = core::slice::from_raw_parts(ptr, len);
        core::str::from_utf8(bytes).unwrap_or("App")
    }
}

// ── Syscall handlers ──────────────────────────────────────────────────────────

/// Create a window for `pid`.  Returns the WM window id on success.
pub unsafe fn create_window(pid: u32, title_bytes: &[u8], width: u32, height: u32) -> i64 {
    if WM_PTR.is_null() { return -1; }

    // Find a free entry slot.
    let slot = 'find: {
        for i in 0..MAX_ENTRIES {
            if !unsafe { ENTRIES[i].active } {
                break 'find Some(i);
            }
        }
        None
    };
    let slot = match slot { Some(s) => s, None => return -12 }; // ENOMEM

    // Copy title into static buffer.
    unsafe {
        let tlen = title_bytes.len().min(TITLE_BUF_LEN - 1);
        TITLE_BUFS[slot][..tlen].copy_from_slice(&title_bytes[..tlen]);
        TITLE_LENS[slot] = tlen;
    }

    // Cascade position based on slot.
    let (screen_w, screen_h) = if GFX_VALID {
        unsafe { (*GFX_PTR).get_dimensions() }
    } else {
        (1280, 800)
    };

    let off     = (slot as u64).min(6) * 24;
    let win_x   = (60 + off).min(screen_w.saturating_sub(width as u64 + 10));
    let win_y   = (70 + off).min(screen_h.saturating_sub(height as u64 + 80));
    let win_w   = (width  as u64).min(screen_w.saturating_sub(win_x + 4));
    let win_h   = (height as u64).min(screen_h.saturating_sub(win_y + 4));

    let title_str: &'static str = unsafe { static_title(slot) };
    let new_win = Window::new(win_x, win_y, win_w, win_h, title_str);

    let wm_id = unsafe {
        match (*WM_PTR).add_window(new_win) {
            Some(id) => id,
            None     => return -12,
        }
    };

    unsafe {
        ENTRIES[slot].active    = true;
        ENTRIES[slot].window_id = wm_id as u32;
        ENTRIES[slot].pid       = pid;
        ENTRIES[slot].head      = 0;
        ENTRIES[slot].tail      = 0;
    }

    wm_id as i64
}

/// Destroy the window owned by `pid` with `window_id`.
pub unsafe fn destroy_window(pid: u32, window_id: u32) -> i64 {
    let slot = match slot_by_pid_and_win(pid, window_id) {
        Some(s) => s,
        None    => return -9, // EBADF
    };
    unsafe {
        if !WM_PTR.is_null() {
            (*WM_PTR).remove_window(window_id as usize);
        }
        ENTRIES[slot].active = false;
    }
    0
}

/// Fill a rectangle in `window_id`'s content area (window-relative coords).
pub unsafe fn fill_rect(
    pid: u32, window_id: u32,
    x: u32, y: u32, w: u32, h: u32, color: u32,
) -> i64 {
    if !GFX_VALID || WM_PTR.is_null() { return -1; }
    if slot_by_pid_and_win(pid, window_id).is_none() { return -9; }

    let (cx, cy, cw, ch) = match content_area(window_id as usize) {
        Some(r) => r,
        None    => return -9,
    };
    let gfx = unsafe { &*GFX_PTR };

    let ax = cx + (x as u64).min(cw);
    let ay = cy + (y as u64).min(ch);
    let aw = (w as u64).min(cw.saturating_sub(x as u64));
    let ah = (h as u64).min(ch.saturating_sub(y as u64));
    if aw > 0 && ah > 0 {
        gfx.fill_rect(ax, ay, aw, ah, color);
    }
    0
}

/// Draw a string in `window_id`'s content area (window-relative coords).
pub unsafe fn draw_text(
    pid: u32, window_id: u32,
    x: u32, y: u32, color: u32,
    text_bytes: &[u8],
) -> i64 {
    if !GFX_VALID || WM_PTR.is_null() { return -1; }
    if slot_by_pid_and_win(pid, window_id).is_none() { return -9; }

    let (cx, cy, cw, ch) = match content_area(window_id as usize) {
        Some(r) => r,
        None    => return -9,
    };
    let gfx = unsafe { &*GFX_PTR };

    let ax = cx + (x as u64).min(cw);
    let ay = cy + (y as u64).min(ch);

    if let Ok(s) = core::str::from_utf8(text_bytes) {
        fonts::draw_string(gfx, ax, ay, s, color);
    }
    0
}

/// Present the window (signals the main loop to include this window in the
/// next blit; the back-buffer is already updated).  Always returns 0.
pub unsafe fn present(_pid: u32, _window_id: u32) -> i64 {
    // The main loop blits the back-buffer every frame.
    // Setting NEEDS_PRESENT just makes the next frame happen promptly.
    unsafe { NEEDS_PRESENT = true; }
    0
}

static mut NEEDS_PRESENT: bool = false;

/// True if any gui-proc window called `present` since last call.
pub unsafe fn take_present_flag() -> bool {
    unsafe {
        let v = NEEDS_PRESENT;
        NEEDS_PRESENT = false;
        v
    }
}

/// Blit a shared memory framebuffer into `window_id`'s content area.
pub unsafe fn blit_shm(
    pid: u32, window_id: u32,
    shm_id: u32,
    src_x: u32, src_y: u32, src_w: u32, src_h: u32,
    dst_x: u32, dst_y: u32,
) -> i64 {
    if !GFX_VALID || WM_PTR.is_null() { return -1; }
    if slot_by_pid_and_win(pid, window_id).is_none() { return -9; }

    let (cx, cy, cw, ch) = match content_area(window_id as usize) {
        Some(r) => r,
        None    => return -9,
    };
    let gfx = unsafe { &*GFX_PTR };

    let phys = shm::seg_phys_base(shm_id as usize);
    if phys == 0 { return -9; }

    const HHDM: u64 = 0xFFFF_8000_0000_0000;
    let base_ptr = (phys + HHDM) as *const u32;

    let blit_w = (src_w as u64).min(cw.saturating_sub(dst_x as u64));
    let blit_h = (src_h as u64).min(ch.saturating_sub(dst_y as u64));
    if blit_w == 0 || blit_h == 0 { return 0; }

    let stride_px = src_w as u64; // stride = source image width in pixels
    for row in 0..blit_h {
        let sy = src_y as u64 + row;
        let dy = dst_y as u64 + row;
        let src_row = unsafe { base_ptr.add((sy * stride_px + src_x as u64) as usize) };
        let abs_x   = cx + dst_x as u64;
        let abs_y   = cy + dy;
        for col in 0..blit_w {
            let pixel = unsafe { src_row.add(col as usize).read_volatile() };
            gfx.put_pixel(abs_x + col, abs_y, pixel);
        }
    }
    0
}

/// Poll for the next event for `window_id`.  Writes to `event_ptr` and returns 0,
/// or returns -6 (EAGAIN) if the queue is empty.
pub unsafe fn poll_event(pid: u32, window_id: u32, event_ptr: u64) -> i64 {
    let slot = match slot_by_pid_and_win(pid, window_id) {
        Some(s) => s,
        None    => return -9,
    };
    let ev = unsafe { ENTRIES[slot].pop() };
    match ev {
        None     => -6, // EAGAIN
        Some(ev) => {
            // Validate user pointer
            if event_ptr < 0x1000 || event_ptr >= 0xFFFF_8000_0000_0000 { return -14; }
            unsafe {
                core::ptr::write_unaligned(event_ptr as *mut GuiEventRaw, ev);
            }
            0
        }
    }
}

/// Write the content area dimensions (w, h) to `w_ptr` and `h_ptr`.
pub unsafe fn get_size(pid: u32, window_id: u32, w_ptr: u64, h_ptr: u64) -> i64 {
    if slot_by_pid_and_win(pid, window_id).is_none() { return -9; }
    let (_, _, cw, ch) = match content_area(window_id as usize) {
        Some(r) => r,
        None    => return -9,
    };
    if w_ptr >= 0x1000 {
        unsafe { core::ptr::write_unaligned(w_ptr as *mut u32, cw as u32); }
    }
    if h_ptr >= 0x1000 {
        unsafe { core::ptr::write_unaligned(h_ptr as *mut u32, ch as u32); }
    }
    0
}

// ── Main-loop event injection ─────────────────────────────────────────────────

/// Push a keyboard char to the event queue of the window with `window_id`.
pub unsafe fn push_key_event(window_id: u32, ch: u8) {
    if let Some(slot) = slot_by_win(window_id) {
        unsafe { ENTRIES[slot].push(GuiEventRaw::key(ch)); }
    }
}

/// Push a mouse-move event (window-relative coords) to `window_id`.
pub unsafe fn push_mouse_move(window_id: u32, rel_x: u16, rel_y: u16) {
    if let Some(slot) = slot_by_win(window_id) {
        unsafe { ENTRIES[slot].push(GuiEventRaw::mouse_move(rel_x, rel_y)); }
    }
}

/// Push a mouse button event to `window_id`.
pub unsafe fn push_mouse_btn(window_id: u32, rel_x: u16, rel_y: u16, button: u8, pressed: bool) {
    if let Some(slot) = slot_by_win(window_id) {
        unsafe { ENTRIES[slot].push(GuiEventRaw::mouse_btn(rel_x, rel_y, button, pressed)); }
    }
}

/// Push a focus-change event to `window_id`.
pub unsafe fn push_focus_event(window_id: u32, gained: bool) {
    if let Some(slot) = slot_by_win(window_id) {
        unsafe { ENTRIES[slot].push(GuiEventRaw::focus(gained)); }
    }
}

/// Push a close event and clean up the entry for `window_id`.
/// Called when the WM removes a process-owned window (e.g. user clicked ✕).
pub unsafe fn on_window_closed(window_id: u32) {
    if let Some(slot) = slot_by_win(window_id) {
        unsafe {
            // Push a close event so the process can exit cleanly.
            ENTRIES[slot].push(GuiEventRaw::close());
            // Mark entry as pending-close (process still alive, window gone).
            // The process will exit after receiving the close event.
        }
    }
}

/// Clean up all entries owned by `pid` (called when a process exits).
pub unsafe fn on_process_exit(pid: u32) {
    unsafe {
        for i in 0..MAX_ENTRIES {
            if ENTRIES[i].active && ENTRIES[i].pid == pid {
                let wid = ENTRIES[i].window_id;
                if !WM_PTR.is_null() {
                    (*WM_PTR).remove_window(wid as usize);
                }
                ENTRIES[i].active = false;
            }
        }
    }
}

// ── Geometry helper ───────────────────────────────────────────────────────────

/// Returns `(content_x, content_y, content_w, content_h)` for `window_id`.
fn content_area(window_id: usize) -> Option<(u64, u64, u64, u64)> {
    unsafe {
        if WM_PTR.is_null() { return None; }
        let win = (*WM_PTR).get_window(window_id)?;
        let cx = win.x;
        let cy = win.y + TITLE_BAR_H;
        let cw = win.width;
        let ch = win.height.saturating_sub(TITLE_BAR_H);
        Some((cx, cy, cw, ch))
    }
}
