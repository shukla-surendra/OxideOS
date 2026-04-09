/// Auto-generated at build time from `assets/wallpaper.png`.
/// If the file was absent, W/H are 0 and PIXELS is empty — callers must guard.
include!(concat!(env!("OUT_DIR"), "/wallpaper_dims.rs"));
pub static PIXELS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper.rgba"));
