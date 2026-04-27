#!/usr/bin/env bash
# install_dep.sh — Install all build and run dependencies for OxideOS on Ubuntu.
# Run with:  bash install_dep.sh
# Tested on Ubuntu 22.04 / 24.04 (x86_64).

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

require_ubuntu() {
    if ! grep -qi ubuntu /etc/os-release 2>/dev/null; then
        warn "This script targets Ubuntu. Proceeding anyway, but your mileage may vary."
    fi
}

require_ubuntu

# ── 1. Core build tools ────────────────────────────────────────────────────────
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

# ── 2. ISO / disk image tools ──────────────────────────────────────────────────
info "Installing ISO and disk image tools..."
sudo apt-get install -y \
    xorriso \
    mtools \
    dosfstools \
    e2fsprogs \
    gdisk \
    fdisk
success "ISO/disk tools installed."

# ── 3. QEMU ───────────────────────────────────────────────────────────────────
info "Installing QEMU..."
sudo apt-get install -y \
    qemu-system-x86 \
    qemu-system-arm \
    qemu-system-misc \
    qemu-utils
success "QEMU installed: $(qemu-system-x86_64 --version | head -1)"

# ── 4. LLVM / LLD ─────────────────────────────────────────────────────────────
# The Rust no_std kernel uses LLD as its linker.  The official LLVM apt script
# installs the latest stable release; a symlink exposes it as plain `lld`.
info "Installing LLVM / LLD..."

LLVM_VERSION=""
if command -v lld &>/dev/null; then
    success "lld is already available: $(lld --version | head -1)"
else
    # Try a recent LLVM version via the official installer
    LLVM_SCRIPT=$(mktemp)
    if curl -fsSL https://apt.llvm.org/llvm.sh -o "$LLVM_SCRIPT" 2>/dev/null; then
        sudo bash "$LLVM_SCRIPT" 20 || sudo bash "$LLVM_SCRIPT" 19 || sudo bash "$LLVM_SCRIPT" 18 || true
        rm -f "$LLVM_SCRIPT"

        # Create a plain `lld` symlink so rustc -Clinker=lld finds it
        for v in 20 19 18 17 16; do
            if command -v "lld-$v" &>/dev/null; then
                LLVM_VERSION=$v
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

# ── 5. Rust via rustup ────────────────────────────────────────────────────────
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

# Source cargo env so rustup/cargo are available in this shell session
# shellcheck source=/dev/null
source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"

# Install Rust nightly (required by kernel/rust-toolchain.toml and userspace/rust-toolchain.toml)
info "Installing Rust nightly toolchain..."
rustup toolchain install nightly --allow-downgrade

# Bare-metal targets used by the kernel
info "Adding bare-metal compilation targets..."
rustup target add --toolchain nightly \
    x86_64-unknown-none \
    aarch64-unknown-none

# Components required by both the kernel and userspace builds
info "Adding rustup components..."
rustup component add --toolchain nightly \
    rust-src \
    llvm-tools-preview

success "Rust nightly toolchain ready: $(rustup run nightly rustc --version)"

# ── 6. Verify key tools ────────────────────────────────────────────────────────
info "Verifying installed tools..."

check() {
    local cmd=$1; local label=${2:-$1}
    if command -v "$cmd" &>/dev/null; then
        success "$label: $(command -v "$cmd")"
    else
        warn "$label not found — some build targets may fail."
    fi
}

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

# ── 7. Add ~/.cargo/bin to PATH permanently ────────────────────────────────────
PROFILE_FILE="$HOME/.bashrc"
if ! grep -q 'cargo/bin' "$PROFILE_FILE" 2>/dev/null; then
    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$PROFILE_FILE"
    info "Added ~/.cargo/bin to PATH in $PROFILE_FILE"
fi

# ── 8. Summary ────────────────────────────────────────────────────────────────
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
warn "Note: musl-programs in userspace/Makefile reference a custom musl-gcc path."
warn "  If 'make' fails for musl targets, either:"
warn "    a) Edit userspace/Makefile: MUSL_GCC := /usr/bin/musl-gcc"
warn "    b) Or build musl from source: https://musl.libc.org/"
