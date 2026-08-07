#!/usr/bin/env bash
# install_dep.sh — Install all build and run dependencies for OxideOS.
# Run with:  bash install_dep.sh
# Tested on Ubuntu 22.04 / 24.04 (x86_64) and macOS (Apple Silicon, Homebrew).

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${CYAN}[info]${NC} $*"; }
success() { echo -e "${GREEN}[ok]${NC}   $*"; }
warn()    { echo -e "${YELLOW}[warn]${NC} $*"; }
die()     { echo -e "${RED}[error]${NC} $*" >&2; exit 1; }

check() {
    local cmd=$1; local label=${2:-$1}
    if command -v "$cmd" &>/dev/null; then
        success "$label: $(command -v "$cmd")"
    else
        warn "$label not found — some build targets may fail."
    fi
}

# ── Rust via rustup (shared by both platforms) ────────────────────────────────
install_rust() {
    info "Setting up Rust via rustup..."

    if command -v rustup &>/dev/null; then
        success "rustup already installed: $(rustup --version)"
    else
        info "Downloading and installing rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- \
            --default-toolchain none \
            --no-modify-path \
            -y
        success "rustup installed."
    fi

    # shellcheck source=/dev/null
    source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"

    # Required by kernel/rust-toolchain.toml and userspace/rust-toolchain.toml
    info "Installing Rust nightly toolchain..."
    rustup toolchain install nightly --allow-downgrade

    info "Adding bare-metal compilation targets..."
    rustup target add --toolchain nightly \
        x86_64-unknown-none \
        aarch64-unknown-none

    info "Adding rustup components..."
    rustup component add --toolchain nightly \
        rust-src \
        llvm-tools-preview

    success "Rust nightly toolchain ready: $(rustup run nightly rustc --version)"
}

# ── Linux (Ubuntu/Debian via apt) ──────────────────────────────────────────────
install_linux() {
    if ! grep -qi ubuntu /etc/os-release 2>/dev/null; then
        warn "This script targets Ubuntu. Proceeding anyway, but your mileage may vary."
    fi

    info "Installing core build tools..."
    sudo apt-get update -qq
    sudo apt-get install -y \
        build-essential \
        git \
        curl \
        wget \
        pkg-config \
        libssl-dev \
        nasm \
        gcc-x86-64-linux-gnu \
        musl-tools
    success "Core build tools installed."

    info "Installing ISO and disk image tools..."
    sudo apt-get install -y \
        xorriso \
        mtools \
        dosfstools \
        e2fsprogs \
        gdisk \
        fdisk
    success "ISO/disk tools installed."

    info "Installing QEMU..."
    sudo apt-get install -y \
        qemu-system-x86 \
        qemu-system-arm \
        qemu-system-misc \
        qemu-system-gui \
        qemu-utils
    success "QEMU installed: $(qemu-system-x86_64 --version | head -1)"

    # The Rust no_std kernel uses LLD as its linker. The official LLVM apt
    # script installs the latest stable release; a symlink exposes it as
    # plain `lld`.
    info "Installing LLVM / LLD..."
    if command -v lld &>/dev/null; then
        success "lld is already available: $(lld --version | head -1)"
    else
        LLVM_SCRIPT=$(mktemp)
        if curl -fsSL https://apt.llvm.org/llvm.sh -o "$LLVM_SCRIPT" 2>/dev/null; then
            sudo bash "$LLVM_SCRIPT" 20 || sudo bash "$LLVM_SCRIPT" 19 || sudo bash "$LLVM_SCRIPT" 18 || true
            rm -f "$LLVM_SCRIPT"

            for v in 20 19 18 17 16; do
                if command -v "lld-$v" &>/dev/null; then
                    sudo ln -sf "/usr/bin/lld-$v" /usr/bin/lld 2>/dev/null || true
                    success "LLD $v installed and symlinked as /usr/bin/lld."
                    break
                fi
            done
        else
            warn "Could not download LLVM installer. Trying apt fallback..."
            sudo apt-get install -y lld || warn "lld not found in apt — Rust linking may fail."
        fi

        if ! command -v lld &>/dev/null; then
            warn "lld not found after install. You may need to create the symlink manually, e.g.:"
            warn "  sudo ln -s /usr/bin/lld-<version> /usr/bin/lld"
        fi
    fi

    install_rust

    info "Verifying installed tools..."
    check nasm           "NASM assembler"
    check xorriso        "xorriso (ISO creation)"
    check mformat        "mtools (mformat)"
    check mcopy          "mtools (mcopy)"
    check mkfs.fat       "dosfstools (mkfs.fat)"
    check mke2fs         "e2fsprogs (mke2fs)"
    check sgdisk         "gdisk (sgdisk)"
    check sfdisk         "util-linux (sfdisk)"
    check qemu-system-x86_64 "QEMU x86_64"
    check lld            "LLD linker"
    check musl-gcc       "musl-gcc (for musl userspace)"
    check x86_64-linux-gnu-gcc "GCC cross-compiler (for C userspace)"
    check cargo          "cargo"
    check rustc          "rustc"
    check git            "git"
    check curl           "curl"

    PROFILE_FILE="$HOME/.bashrc"
    if ! grep -q 'cargo/bin' "$PROFILE_FILE" 2>/dev/null; then
        echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$PROFILE_FILE"
        info "Added ~/.cargo/bin to PATH in $PROFILE_FILE"
    fi

    echo ""
    echo -e "${GREEN}All dependencies installed.${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Reload your shell (or run: source ~/.cargo/env)"
    echo "  2. Build the kernel and create a bootable ISO:"
    echo "       make"
    echo "  3. Run in QEMU:"
    echo "       make run"
    echo "     or with SDL display:"
    echo "       make run-gui"
    echo ""
    echo "Optional disk images:"
    echo "  make disk      # FAT16 persistent storage"
    echo "  make ext2-disk # ext2 secondary drive"
    echo ""
}

# ── macOS (Homebrew) ────────────────────────────────────────────────────────────
install_macos() {
    if ! command -v brew &>/dev/null; then
        die "Homebrew not found. Install it from https://brew.sh, then re-run this script."
    fi

    info "Installing core build tools, ISO/disk tools, QEMU, and LLD via Homebrew..."
    brew install \
        git \
        curl \
        wget \
        pkg-config \
        nasm \
        xorriso \
        mtools \
        dosfstools \
        qemu \
        lld
    success "Core/ISO/disk/QEMU/LLD tools installed."

    # macOS has no native Linux cross-gcc equivalent to Ubuntu's
    # gcc-x86-64-linux-gnu / musl-tools. FiloSottile's musl-cross tap builds
    # real cross gcc toolchains (x86_64-linux-musl-gcc, aarch64-linux-musl-gcc,
    # ...) that userspace/Makefile uses in place of both — see the comment
    # there for why a musl-targeted gcc is fine even for the plain
    # "linux-gnu" freestanding programs.
    info "Installing musl cross-toolchain (FiloSottile/musl-cross)..."
    if command -v x86_64-linux-musl-gcc &>/dev/null; then
        success "x86_64-linux-musl-gcc already available: $(command -v x86_64-linux-musl-gcc)"
    else
        brew install FiloSottile/musl-cross/musl-cross
        success "musl-cross installed."
    fi

    install_rust

    info "Verifying installed tools..."
    check nasm                   "NASM assembler"
    check xorriso                "xorriso (ISO creation)"
    check mformat                "mtools (mformat)"
    check mcopy                  "mtools (mcopy)"
    check mkfs.fat                "dosfstools (mkfs.fat)"
    check qemu-system-x86_64     "QEMU x86_64"
    check qemu-system-aarch64    "QEMU aarch64"
    check ld.lld                 "LLD linker"
    check x86_64-linux-musl-gcc  "musl cross-gcc (C/musl userspace)"
    check cargo                  "cargo"
    check rustc                  "rustc"
    check git                    "git"
    check curl                   "curl"

    warn "e2fsprogs (mke2fs) and gdisk/util-linux (sgdisk/sfdisk) have no"
    warn "  Homebrew equivalent used here — 'make ext2-disk' and"
    warn "  'make install-image' are Linux-only for now."

    LLD_BIN="$(brew --prefix lld 2>/dev/null)/bin"
    for PROFILE_FILE in "$HOME/.zshrc" "$HOME/.bashrc"; do
        [ -f "$PROFILE_FILE" ] || continue
        if ! grep -q 'cargo/bin' "$PROFILE_FILE" 2>/dev/null; then
            echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$PROFILE_FILE"
            info "Added ~/.cargo/bin to PATH in $PROFILE_FILE"
        fi
        if [ -n "$LLD_BIN" ] && ! grep -qF "$LLD_BIN" "$PROFILE_FILE" 2>/dev/null; then
            echo "export PATH=\"$LLD_BIN:\$PATH\"" >> "$PROFILE_FILE"
            info "Added $LLD_BIN to PATH in $PROFILE_FILE"
        fi
    done

    echo ""
    echo -e "${GREEN}All dependencies installed.${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Reload your shell, or for this session run:"
    echo "       source ~/.cargo/env"
    echo "       export PATH=\"$LLD_BIN:\$PATH\""
    echo "  2. Use the nightly toolchain in this repo:"
    echo "       rustup override set nightly"
    echo "  3. Build the kernel and create a bootable ISO:"
    echo "       make all"
    echo "  4. Run in QEMU:"
    echo "       make run          # GUI, cocoa display (SDL is unavailable on Homebrew's qemu)"
    echo "       make run-bios     # headless, serial output — fastest for iteration"
    echo ""
    echo "Optional disk images:"
    echo "  make disk      # FAT16 persistent storage"
    echo ""
    warn "Note: 'make' alone runs the 'setup' target (this script) because it's"
    warn "  first in the Makefile — use 'make all' to build."
}

# ── Dispatch ────────────────────────────────────────────────────────────────────
case "$(uname -s)" in
    Darwin) install_macos ;;
    Linux)  install_linux ;;
    *)      die "Unsupported OS: $(uname -s). This script supports Linux (Ubuntu) and macOS." ;;
esac
