//! OxideOS version information — single source of truth.
//!
//! # How to cut a new release
//!
//! 1. Edit `Cargo.toml`  → bump `version = "X.Y.Z"`
//! 2. Edit `build.rs`    → update `pre` and `codename` in `emit_version_info()`
//!
//! Every constant here is derived from those two files at compile time.
//! Nothing else in the codebase should contain literal version strings.

// ── Primitives (all set by build.rs / Cargo.toml) ────────────────────────────

/// Base semver, e.g. `"0.1.0"`.  Source: `Cargo.toml version`.
pub const SEMVER:      &str = env!("CARGO_PKG_VERSION");

/// Pre-release suffix, e.g. `"-dev"`, `"-rc.1"`, or `""` for stable.
/// Source: `build.rs` → `OXIDE_PRE`.
pub const PRE:         &str = env!("OXIDE_PRE");

/// Release codename, e.g. `"Iron"`.  Source: `build.rs` → `OXIDE_CODENAME`.
pub const CODENAME:    &str = env!("OXIDE_CODENAME");

/// Build date, e.g. `"2026-04-27"`.  Source: `build.rs` → `OXIDE_BUILD_DATE`.
pub const BUILD_DATE:  &str = env!("OXIDE_BUILD_DATE");

// ── Stable metadata ───────────────────────────────────────────────────────────
pub const NAME:        &str = "OxideOS";
pub const ARCH:        &str = "x86_64";
pub const BOOTLOADER:  &str = "Limine v9";
pub const KERNEL_LANG: &str = "Rust (no_std)";

// ── Composed version strings (derived, do NOT edit directly) ──────────────────

/// `"0.1.0-dev"` — semver + pre-release tag.
pub const VERSION:     &str = env!("OXIDE_VERSION");

/// `"v0.1.0-dev"` — version string with `v` prefix.
pub const V_VERSION:   &str = concat!("v", env!("OXIDE_VERSION"));

/// `"OxideOS v0.1.0-dev"` — name + version.
pub const FULL:        &str = concat!("OxideOS v", env!("OXIDE_VERSION"));

/// `"OxideOS v0.1.0-dev \"Iron\""` — name + version + codename.
pub const FULL_NAMED:  &str = concat!("OxideOS v", env!("OXIDE_VERSION"),
                                      " \"", env!("OXIDE_CODENAME"), "\"");

/// Contents for `/proc/version`.
pub const PROC_VERSION: &str = env!("OXIDE_PROC_VERSION");

/// Contents for `/etc/version`.
pub const ETC_VERSION:  &str = env!("OXIDE_ETC_VERSION");

// ── Display helpers ───────────────────────────────────────────────────────────

/// Short form for the taskbar / sysinfo: `"OxideOS  v0.1.0-dev"` (double space).
pub const DISPLAY_NAME_VER: &str = concat!("OxideOS  v", env!("OXIDE_VERSION"));

/// Architecture + bootloader line for the sysinfo panel.
pub const DISPLAY_ARCH_LINE: &str = concat!("x86_64  ", env!("OXIDE_CODENAME"), " release");
