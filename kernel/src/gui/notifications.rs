//! Toast notification manager for OxideOS.
//! Displays up to 5 stacked cards in the top-right corner, auto-dismissing after a few seconds.

use crate::gui::graphics::Graphics;
use crate::gui::fonts;

const MAX_NOTIFS:      usize = 5;
const NOTIF_W:         u64   = 300;
const NOTIF_H:         u64   = 72;
const NOTIF_GAP:       u64   = 8;
const NOTIF_MARGIN:    u64   = 16;
const NOTIF_TOP:       u64   = 56; // just below the 48px taskbar
const NOTIF_DURATION:  u32   = 500; // ~5 s at 100 Hz

#[derive(Copy, Clone)]
struct Notification {
    title:       [u8; 48],
    title_len:   usize,
    body:        [u8; 80],
    body_len:    usize,
    ticks:       u32,
    icon_color:  u32,
}

impl Notification {
    const fn empty() -> Self {
        Self {
            title:      [0; 48],
            title_len:  0,
            body:       [0; 80],
            body_len:   0,
            ticks:      0,
            icon_color: 0xFF5294E2,
        }
    }
}

pub struct NotificationManager {
    slots: [Option<Notification>; MAX_NOTIFS],
}

impl NotificationManager {
    pub const fn new() -> Self {
        Self { slots: [None; MAX_NOTIFS] }
    }

    /// Push a new notification. Oldest is evicted if all slots are full.
    pub fn push(&mut self, title: &str, body: &str, icon_color: u32) {
        let mut n = Notification::empty();
        n.icon_color = icon_color;
        n.ticks = NOTIF_DURATION;

        let tb = title.as_bytes();
        let tl = tb.len().min(47);
        n.title[..tl].copy_from_slice(&tb[..tl]);
        n.title_len = tl;

        let bb = body.as_bytes();
        let bl = bb.len().min(79);
        n.body[..bl].copy_from_slice(&bb[..bl]);
        n.body_len = bl;

        // Find a free slot
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(n);
                return;
            }
        }
        // All full — shift out oldest, append new at end
        for i in 0..MAX_NOTIFS - 1 {
            self.slots[i] = self.slots[i + 1];
        }
        self.slots[MAX_NOTIFS - 1] = Some(n);
    }

    /// Countdown all active timers. Returns `true` if anything expired (needs redraw).
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        for slot in self.slots.iter_mut() {
            if let Some(n) = slot {
                if n.ticks > 0 {
                    n.ticks -= 1;
                    if n.ticks == 0 {
                        *slot = None;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub fn any_active(&self) -> bool {
        self.slots.iter().any(|s| s.is_some())
    }

    pub fn draw(&self, graphics: &Graphics, screen_w: u64) {
        let x = screen_w.saturating_sub(NOTIF_W + NOTIF_MARGIN);
        let mut y = NOTIF_TOP;
        for slot in &self.slots {
            if let Some(n) = slot {
                draw_toast(graphics, x, y, n);
                y += NOTIF_H + NOTIF_GAP;
            }
        }
    }
}

fn draw_toast(graphics: &Graphics, x: u64, y: u64, n: &Notification) {
    // Shadow
    graphics.draw_soft_shadow(x, y, NOTIF_W, NOTIF_H, 8, 0x50);

    // Card background
    graphics.fill_rounded_rect(x, y, NOTIF_W, NOTIF_H, 10, 0xFF383838);
    graphics.draw_rounded_rect(x, y, NOTIF_W, NOTIF_H, 10, 0xFF505050, 1);

    // Left color accent strip
    graphics.fill_rounded_rect(x + 1, y + 2, 4, NOTIF_H - 4, 2, n.icon_color);

    // Icon circle
    let icon_y = y + (NOTIF_H - 20) / 2;
    graphics.fill_rounded_rect(x + 14, icon_y, 20, 20, 10, n.icon_color);

    // Title
    if n.title_len > 0 {
        if let Ok(s) = core::str::from_utf8(&n.title[..n.title_len]) {
            fonts::draw_string(graphics, x + 42, y + 14, s, 0xFFEEEEEE);
        }
    }

    // Body
    if n.body_len > 0 {
        if let Ok(s) = core::str::from_utf8(&n.body[..n.body_len]) {
            fonts::draw_string(graphics, x + 42, y + 30, s, 0xFF999999);
        }
    }

    // Progress bar (time remaining)
    let bar_fill = n.ticks as u64 * (NOTIF_W - 20) / NOTIF_DURATION as u64;
    graphics.fill_rect(x + 10, y + NOTIF_H - 6, NOTIF_W - 20, 3, 0xFF444444);
    if bar_fill > 0 {
        graphics.fill_rounded_rect(x + 10, y + NOTIF_H - 6, bar_fill, 3, 1, n.icon_color);
    }
}
