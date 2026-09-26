// src/gui/mod.rs - GUI system for OxideOS

pub mod oxide_backend;
// The PS/2 mouse is a driver, not a desktop component; re-exported so
// `crate::gui::mouse` keeps working for the desktop code.
pub use crate::kernel::drivers::mouse;
pub mod graphics;
pub mod colors;
pub mod fonts;
pub mod widgets;
pub mod window_manager;
pub mod text_editor;
pub mod terminal;
pub mod launcher;
pub mod start_menu;
pub mod menu;
pub mod notepad;
pub mod overview;
pub mod notifications;
pub mod quick_settings;
pub mod calendar;
