//! oxide-widgets — UI widget library for OxideOS user-space programs.
//!
//! Wraps the raw GUI syscalls from oxide-rt into reusable stateful widgets.
//!
//! # Quick start
//! ```no_run
//! use oxide_widgets::{Canvas, Theme, Button, Label, ProgressBar, Rect};
//! use oxide_rt::gui_create;
//!
//! let win = gui_create("My App", 640, 480).unwrap();
//! let mut canvas = Canvas::new(win);
//! let mut btn = Button::new(Rect::new(10, 10, 120, 30), "Click me");
//!
//! loop {
//!     canvas.clear(Theme::DARK.bg);
//!     btn.draw(&mut canvas);
//!     canvas.present();
//!     // handle events ...
//! }
//! ```

#![no_std]

use oxide_rt::{
    GuiWindow, GuiEvent,
    gui_fill_rect, gui_draw_text, gui_present, gui_poll_event, gui_get_size,
};

// ── Rect ──────────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    #[inline] pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self { Self { x, y, w, h } }
    #[inline] pub const fn right(&self)  -> u32 { self.x + self.w }
    #[inline] pub const fn bottom(&self) -> u32 { self.y + self.h }

    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    pub fn inset(&self, dx: u32, dy: u32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            w: self.w.saturating_sub(dx * 2),
            h: self.h.saturating_sub(dy * 2),
        }
    }
}

// ── Theme ─────────────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct Theme {
    pub bg:          u32,
    pub panel:       u32,
    pub border:      u32,
    pub text:        u32,
    pub text_dim:    u32,
    pub accent:      u32,
    pub btn_normal:  u32,
    pub btn_hover:   u32,
    pub btn_press:   u32,
    pub btn_text:    u32,
    pub progress_bg: u32,
    pub progress_fg: u32,
    pub input_bg:    u32,
    pub input_focus: u32,
    pub selection:   u32,
}

impl Theme {
    pub const DARK: Self = Self {
        bg:          0xFF0C0C0C,
        panel:       0xFF161B22,
        border:      0xFF2A2A2A,
        text:        0xFFCDD9E5,
        text_dim:    0xFF636E7B,
        accent:      0xFF4EC9B0,
        btn_normal:  0xFF21262D,
        btn_hover:   0xFF30363D,
        btn_press:   0xFF1A3A5C,
        btn_text:    0xFFCDD9E5,
        progress_bg: 0xFF161B22,
        progress_fg: 0xFF4EC9B0,
        input_bg:    0xFF0D1117,
        input_focus: 0xFF1F6FEB,
        selection:   0xFF1A3A5C,
    };

    pub const LIGHT: Self = Self {
        bg:          0xFFF6F8FA,
        panel:       0xFFEAECF0,
        border:      0xFFD0D7DE,
        text:        0xFF1F2328,
        text_dim:    0xFF656D76,
        accent:      0xFF0969DA,
        btn_normal:  0xFFEAECF0,
        btn_hover:   0xFFD0D7DE,
        btn_press:   0xFFBBDEFB,
        btn_text:    0xFF1F2328,
        progress_bg: 0xFFEAECF0,
        progress_fg: 0xFF0969DA,
        input_bg:    0xFFFFFFFF,
        input_focus: 0xFF0969DA,
        selection:   0xFFBBDEFB,
    };
}

// ── Canvas ────────────────────────────────────────────────────────────────────

/// Drawing context wrapping a GuiWindow.
#[derive(Copy, Clone)]
pub struct Canvas {
    pub win: GuiWindow,
}

impl Canvas {
    pub fn new(win: GuiWindow) -> Self { Self { win } }

    pub fn width(&self)  -> u32 { self.win.width }
    pub fn height(&self) -> u32 { self.win.height }

    /// Refresh the window's size from the kernel.
    pub fn sync_size(&mut self) {
        let (w, h) = gui_get_size(self.win);
        self.win.width  = w;
        self.win.height = h;
    }

    /// Fill the entire canvas with `color`.
    pub fn clear(&self, color: u32) {
        gui_fill_rect(self.win, 0, 0, self.win.width, self.win.height, color);
    }

    pub fn fill_rect(&self, r: Rect, color: u32) {
        gui_fill_rect(self.win, r.x, r.y, r.w, r.h, color);
    }

    pub fn draw_text(&self, x: u32, y: u32, color: u32, text: &str) {
        gui_draw_text(self.win, x, y, color, text);
    }

    /// Draw a 1-px border around `r`.
    pub fn draw_border(&self, r: Rect, color: u32) {
        // top / bottom
        gui_fill_rect(self.win, r.x, r.y,              r.w, 1, color);
        gui_fill_rect(self.win, r.x, r.y + r.h.saturating_sub(1), r.w, 1, color);
        // left / right
        gui_fill_rect(self.win, r.x,                       r.y, 1, r.h, color);
        gui_fill_rect(self.win, r.x + r.w.saturating_sub(1), r.y, 1, r.h, color);
    }

    /// Draw text centered horizontally within `r`.
    pub fn draw_text_centered(&self, r: Rect, color: u32, text: &str) {
        const CHAR_W: u32 = 9;
        let text_w = text.len() as u32 * CHAR_W;
        let tx = r.x + (r.w.saturating_sub(text_w)) / 2;
        let ty = r.y + (r.h.saturating_sub(16)) / 2;
        gui_draw_text(self.win, tx, ty, color, text);
    }

    pub fn present(&self) {
        gui_present(self.win);
    }

    pub fn poll_event(&self) -> Option<GuiEvent> {
        gui_poll_event(self.win)
    }
}

// ── Label ─────────────────────────────────────────────────────────────────────

pub struct Label {
    pub rect:  Rect,
    pub text:  &'static str,
    pub color: u32,
}

impl Label {
    pub const fn new(rect: Rect, text: &'static str, color: u32) -> Self {
        Self { rect, text, color }
    }

    pub fn draw(&self, canvas: &Canvas) {
        canvas.draw_text(self.rect.x, self.rect.y + 2, self.color, self.text);
    }
}

// ── Button ────────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq)]
pub enum BtnState { Normal, Hover, Pressed }

pub struct Button {
    pub rect:    Rect,
    pub label:   &'static str,
    pub state:   BtnState,
    /// Set to `true` on the frame a click is confirmed (pressed → released over button).
    pub clicked: bool,
}

impl Button {
    pub const fn new(rect: Rect, label: &'static str) -> Self {
        Self { rect, label, state: BtnState::Normal, clicked: false }
    }

    /// Feed a GuiEvent to update hover/click state.  Returns `true` if clicked.
    pub fn handle_event(&mut self, ev: &GuiEvent) -> bool {
        self.clicked = false;
        match ev.kind {
            GuiEvent::MOUSE_MOVE => {
                if let Some((x, y)) = ev.as_mouse_move() {
                    if self.rect.contains(x as u32, y as u32) {
                        if self.state != BtnState::Pressed {
                            self.state = BtnState::Hover;
                        }
                    } else {
                        if self.state != BtnState::Pressed {
                            self.state = BtnState::Normal;
                        }
                    }
                }
            }
            GuiEvent::MOUSE_BTN => {
                if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
                    if pressed {
                        if self.rect.contains(x as u32, y as u32) {
                            self.state = BtnState::Pressed;
                        }
                    } else {
                        if self.state == BtnState::Pressed {
                            if self.rect.contains(x as u32, y as u32) {
                                self.clicked = true;
                            }
                            self.state = if self.rect.contains(x as u32, y as u32) {
                                BtnState::Hover
                            } else {
                                BtnState::Normal
                            };
                        }
                    }
                }
            }
            _ => {}
        }
        self.clicked
    }

    pub fn draw(&self, canvas: &Canvas, theme: &Theme) {
        let bg = match self.state {
            BtnState::Normal  => theme.btn_normal,
            BtnState::Hover   => theme.btn_hover,
            BtnState::Pressed => theme.btn_press,
        };
        canvas.fill_rect(self.rect, bg);
        canvas.draw_border(self.rect, theme.border);
        canvas.draw_text_centered(self.rect, theme.btn_text, self.label);
    }
}

// ── ProgressBar ───────────────────────────────────────────────────────────────

pub struct ProgressBar {
    pub rect:    Rect,
    /// 0–100
    pub percent: u32,
}

impl ProgressBar {
    pub const fn new(rect: Rect) -> Self { Self { rect, percent: 0 } }

    pub fn set(&mut self, percent: u32) { self.percent = percent.min(100); }

    pub fn draw(&self, canvas: &Canvas, theme: &Theme) {
        canvas.fill_rect(self.rect, theme.progress_bg);
        canvas.draw_border(self.rect, theme.border);
        if self.percent > 0 {
            let filled_w = self.rect.w.saturating_sub(2) * self.percent / 100;
            if filled_w > 0 {
                canvas.fill_rect(
                    Rect::new(self.rect.x + 1, self.rect.y + 1, filled_w, self.rect.h.saturating_sub(2)),
                    theme.progress_fg,
                );
            }
        }
    }
}

// ── Checkbox ──────────────────────────────────────────────────────────────────

pub struct Checkbox {
    pub rect:    Rect,
    pub label:   &'static str,
    pub checked: bool,
}

impl Checkbox {
    pub const fn new(rect: Rect, label: &'static str) -> Self {
        Self { rect, label, checked: false }
    }

    /// Returns `true` if the toggle state changed.
    pub fn handle_event(&mut self, ev: &GuiEvent) -> bool {
        if ev.kind == GuiEvent::MOUSE_BTN {
            if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
                if !pressed && self.rect.contains(x as u32, y as u32) {
                    self.checked = !self.checked;
                    return true;
                }
            }
        }
        false
    }

    pub fn draw(&self, canvas: &Canvas, theme: &Theme) {
        let box_rect = Rect::new(self.rect.x, self.rect.y + (self.rect.h.saturating_sub(14)) / 2, 14, 14);
        canvas.fill_rect(box_rect, theme.input_bg);
        canvas.draw_border(box_rect, theme.border);
        if self.checked {
            canvas.fill_rect(box_rect.inset(3, 3), theme.accent);
        }
        canvas.draw_text(self.rect.x + 20, self.rect.y + 2, theme.text, self.label);
    }
}

// ── TextInput ─────────────────────────────────────────────────────────────────

pub struct TextInput<const N: usize> {
    pub rect:    Rect,
    pub focused: bool,
    buf:  [u8; N],
    len:  usize,
}

impl<const N: usize> TextInput<N> {
    pub const fn new(rect: Rect) -> Self {
        Self { rect, focused: false, buf: [0u8; N], len: 0 }
    }

    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    pub fn clear(&mut self) { self.len = 0; }

    pub fn handle_event(&mut self, ev: &GuiEvent) {
        match ev.kind {
            GuiEvent::MOUSE_BTN => {
                if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
                    if pressed {
                        self.focused = self.rect.contains(x as u32, y as u32);
                    }
                }
            }
            GuiEvent::KEY if self.focused => {
                let ch = ev.data[0];
                if ch == 8 || ch == 127 {
                    if self.len > 0 { self.len -= 1; }
                } else if ch >= 32 && ch < 127 && self.len < N {
                    self.buf[self.len] = ch;
                    self.len += 1;
                }
            }
            _ => {}
        }
    }

    pub fn draw(&self, canvas: &Canvas, theme: &Theme) {
        let bg = if self.focused { theme.input_focus } else { theme.input_bg };
        canvas.fill_rect(self.rect, theme.input_bg);
        canvas.draw_border(self.rect, if self.focused { theme.input_focus } else { theme.border });
        let inner = self.rect.inset(4, 3);
        canvas.draw_text(inner.x, inner.y, theme.text, self.text());
        if self.focused {
            let cursor_x = inner.x + self.len as u32 * 9;
            if cursor_x < self.rect.right() {
                canvas.fill_rect(Rect::new(cursor_x, inner.y, 2, 14), bg);
            }
        }
    }
}

// ── Scrollbar ─────────────────────────────────────────────────────────────────

pub struct Scrollbar {
    pub rect:      Rect,
    pub total:     u32,
    pub visible:   u32,
    pub offset:    u32,
    dragging:      bool,
    drag_start_y:  u32,
    drag_start_off: u32,
}

impl Scrollbar {
    pub const fn new(rect: Rect) -> Self {
        Self { rect, total: 1, visible: 1, offset: 0, dragging: false,
               drag_start_y: 0, drag_start_off: 0 }
    }

    fn thumb_rect(&self) -> Rect {
        if self.total == 0 || self.visible >= self.total {
            return self.rect;
        }
        let track_h = self.rect.h;
        let thumb_h = (track_h * self.visible / self.total).max(16);
        let thumb_y = self.rect.y + (track_h - thumb_h) * self.offset / (self.total - self.visible);
        Rect::new(self.rect.x, thumb_y, self.rect.w, thumb_h)
    }

    pub fn handle_event(&mut self, ev: &GuiEvent) {
        match ev.kind {
            GuiEvent::MOUSE_BTN => {
                if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
                    if pressed && self.rect.contains(x as u32, y as u32) {
                        self.dragging = true;
                        self.drag_start_y   = y as u32;
                        self.drag_start_off = self.offset;
                    } else {
                        self.dragging = false;
                    }
                }
            }
            GuiEvent::MOUSE_MOVE if self.dragging => {
                if let Some((_x, y)) = ev.as_mouse_move() {
                    let dy = y as i32 - self.drag_start_y as i32;
                    let track_h = self.rect.h as i32;
                    if track_h > 0 && self.total > self.visible {
                        let delta = dy * (self.total - self.visible) as i32 / track_h;
                        let new_off = (self.drag_start_off as i32 + delta)
                            .max(0)
                            .min((self.total - self.visible) as i32) as u32;
                        self.offset = new_off;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn draw(&self, canvas: &Canvas, theme: &Theme) {
        canvas.fill_rect(self.rect, theme.panel);
        if self.total > self.visible {
            canvas.fill_rect(self.thumb_rect(), theme.border);
        }
    }
}

// ── fmt_u32 ───────────────────────────────────────────────────────────────────

/// Format `n` into `buf` as a decimal string; returns the written slice.
pub fn fmt_u32(n: u32, buf: &mut [u8; 20]) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap();
    }
    let mut tmp = [0u8; 20];
    let mut pos = 20usize;
    let mut v = n;
    while v > 0 {
        pos -= 1;
        tmp[pos] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let digits = &tmp[pos..];
    buf[..digits.len()].copy_from_slice(digits);
    core::str::from_utf8(&buf[..digits.len()]).unwrap()
}

/// Format `n` as "X.Y%" into `buf`.
pub fn fmt_percent(numerator: u32, denominator: u32, buf: &mut [u8; 24]) -> &str {
    if denominator == 0 {
        buf[..2].copy_from_slice(b"0%");
        return core::str::from_utf8(&buf[..2]).unwrap();
    }
    let pct = numerator * 100 / denominator;
    let frac = numerator * 1000 / denominator % 10;
    let mut tmp = [0u8; 24];
    let mut pos = 24usize;
    pos -= 1; tmp[pos] = b'%';
    pos -= 1; tmp[pos] = b'0' + frac as u8;
    pos -= 1; tmp[pos] = b'.';
    let mut v = pct;
    if v == 0 {
        pos -= 1; tmp[pos] = b'0';
    } else {
        while v > 0 {
            pos -= 1;
            tmp[pos] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }
    let len = 24 - pos;
    buf[..len].copy_from_slice(&tmp[pos..]);
    core::str::from_utf8(&buf[..len]).unwrap()
}
