//! OxideOS File Manager — entry point.
//!
//! Module map
//! ─────────────────────────────────────────────────────────────────────────────
//! constants  All compile-time values: window size, grid metrics, colors,
//!            and sidebar section Y positions.
//!
//! fixstr     `FixStr<N>` — heap-free fixed-capacity UTF-8 string.
//!
//! types      Domain types: `EscState`, `Layout`, `DirEntry`, `SidebarItem`,
//!            `SidebarHit`, `SIDEBAR_ITEMS`.
//!
//! app        `App` struct + navigation, scroll, and event-handling methods.
//!
//! render     `impl App { fn draw }` — all GUI rendering logic.
//! ─────────────────────────────────────────────────────────────────────────────
#![no_std]
#![no_main]

mod constants;
mod fixstr;
mod types;
mod app;
mod render;

use oxide_rt::{exit, sleep_ms, gui_create, gui_poll_event};
use app::App;
use constants::{WIN_W_INIT, WIN_H_INIT};

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let win = match gui_create("File Manager", WIN_W_INIT, WIN_H_INIT) {
        Some(w) => w,
        None    => exit(1),
    };

    let mut app = App::new(win);

    loop {
        app.sync_size();

        loop {
            let Some(ev) = gui_poll_event(app.win) else { break };
            app.handle_event(ev);
        }

        if app.dirty {
            app.draw();
            app.dirty = false;
        }

        sleep_ms(16); // ~60 fps cap
    }
}
