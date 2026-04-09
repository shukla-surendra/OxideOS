fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    // Tell cargo to pass the linker script to the linker..
    println!("cargo:rustc-link-arg=-Tlinker-{arch}.ld");
    // ..and to re-run if it changes.
    println!("cargo:rerun-if-changed=linker-{arch}.ld");

    encode_wallpaper();
}

/// Decode `assets/wallpaper.png` (if present) into a raw RGBA blob + a small
/// Rust source file with the image dimensions.  Both land in `$OUT_DIR` so the
/// kernel can `include_bytes!` / `include!` them without any runtime PNG library.
///
/// If the file is absent or unsupported, zero-sized stubs are written instead
/// so the kernel still compiles and gracefully falls back to a gradient.
fn encode_wallpaper() {
    let out_dir  = std::env::var("OUT_DIR").unwrap();
    let src_path = "assets/wallpaper.png";
    println!("cargo:rerun-if-changed={src_path}");

    let (width, height, pixels) = load_png(src_path).unwrap_or((0, 0, vec![]));

    std::fs::write(format!("{out_dir}/wallpaper.rgba"), &pixels)
        .expect("failed to write wallpaper.rgba");

    std::fs::write(
        format!("{out_dir}/wallpaper_dims.rs"),
        format!(
            "pub const WALLPAPER_W: u32 = {width};\npub const WALLPAPER_H: u32 = {height};\n"
        ),
    )
    .expect("failed to write wallpaper_dims.rs");

    if width > 0 {
        println!("cargo:warning=wallpaper: embedded {width}x{height} ({} KiB raw RGBA)",
                 pixels.len() / 1024);
    } else {
        println!("cargo:warning=wallpaper: assets/wallpaper.png not found — Image style will fall back to Default");
    }
}

/// Returns `(width, height, rgba_bytes)` or an error string.
fn load_png(path: &str) -> Result<(u32, u32, Vec<u8>), String> {
    use std::fs::File;
    use std::io::BufReader;

    if !std::path::Path::new(path).exists() {
        return Err(format!("{path} not found"));
    }

    let file    = File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info    = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    let w = info.width;
    let h = info.height;
    let raw = &buf[..info.buffer_size()];

    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb  => raw.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => raw.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale      => raw.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        _ => return Err(format!("unsupported PNG colour type {:?}", info.color_type)),
    };

    Ok((w, h, rgba))
}
