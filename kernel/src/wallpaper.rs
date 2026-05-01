/// Auto-generated at build time from `assets/*.png`.
/// WALLPAPER_DIMS[i] = (width, height) — zero if the source was absent.
include!(concat!(env!("OUT_DIR"), "/wallpaper_dims.rs"));

pub static PIXELS_DEFAULT:     &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper_default.rgba"));
pub static PIXELS_DARK:        &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper_dark.rgba"));
pub static PIXELS_BLUE_PANDAS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper_blue_pandas.rgba"));
pub static PIXELS_DARK_RABBIT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper_dark_rabbit.rgba"));
pub static PIXELS_PANDAS_LIGHT:&[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpaper_pandas_light.rgba"));

/// Index order must match `WALLPAPER_DIMS` and the build.rs slug list.
pub const ALL_PIXELS: [&'static [u8]; 5] = [
    PIXELS_DEFAULT,
    PIXELS_DARK,
    PIXELS_BLUE_PANDAS,
    PIXELS_DARK_RABBIT,
    PIXELS_PANDAS_LIGHT,
];
