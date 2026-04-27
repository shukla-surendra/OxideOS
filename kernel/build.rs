fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    println!("cargo:rustc-link-arg=-Tlinker-{arch}.ld");
    println!("cargo:rerun-if-changed=linker-{arch}.ld");

    emit_version_info();
    encode_wallpaper();
    probe_optional_binaries();
}

// ── OxideOS version information ───────────────────────────────────────────────
//
// THIS IS THE ONE PLACE TO EDIT FOR A NEW RELEASE:
//   1. Bump `version` in Cargo.toml  (semver, e.g. "0.2.0")
//   2. Update `PRE` below            ("" = stable, "-dev", "-alpha", "-rc.1" …)
//   3. Update `CODENAME` below       (one word, capitalised)
//
// Everything else (composed strings, /proc/version, /etc/version, GUI panels)
// is derived automatically at compile time.
// ─────────────────────────────────────────────────────────────────────────────
fn emit_version_info() {
    // ┌── EDIT ONLY THESE TWO LINES FOR A NEW RELEASE ────────────────────────┐
    let pre      = "-dev";    // "" | "-alpha" | "-beta" | "-rc.1" | "-dev"
    let codename = "Iron";    // Release codename (capitalised single word)
    // └────────────────────────────────────────────────────────────────────────┘

    let semver = std::env::var("CARGO_PKG_VERSION").unwrap();
    let build_date = current_date();

    // Full version string, e.g. "0.1.0-dev"
    let version_full = format!("{semver}{pre}");
    // /proc/version style string
    let proc_version = format!("OxideOS version {version_full} (rust-nightly) #1 {build_date}\n");
    // /etc/version style string
    let etc_version  = format!("OxideOS {version_full} \"{codename}\"\n");

    println!("cargo:rustc-env=OXIDE_PRE={pre}");
    println!("cargo:rustc-env=OXIDE_CODENAME={codename}");
    println!("cargo:rustc-env=OXIDE_BUILD_DATE={build_date}");
    println!("cargo:rustc-env=OXIDE_VERSION={version_full}");
    println!("cargo:rustc-env=OXIDE_PROC_VERSION={proc_version}");
    println!("cargo:rustc-env=OXIDE_ETC_VERSION={etc_version}");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

/// Returns today's date as "YYYY-MM-DD" using simple arithmetic on the Unix epoch.
fn current_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Days since epoch
    let mut days = (secs / 86400) as u32;
    let mut year = 1970u32;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let months = if is_leap(year) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut month = 1u32;
    for &dm in &months {
        if days < dm { break; }
        days -= dm;
        month += 1;
    }
    format!("{year:04}-{month:02}-{:02}", days + 1)
}

fn is_leap(y: u32) -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) }

fn probe_optional_binaries() {
    for name in &["lua", "busybox", "bash"] {
        let path = format!("../userspace/bin/{}.elf", name);
        println!("cargo:rerun-if-changed={path}");
        if std::path::Path::new(&path).exists() {
            println!("cargo:rustc-cfg=has_{name}");
        }
    }
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
