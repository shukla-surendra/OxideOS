#!/usr/bin/env bash
# set -e  # exit on first error

TARGET="x86_32-oxideos.json"
KERNEL_NAME="OxideOs"
BUILD_DIR="os_iso_configuration"
ISO_NAME="iso_builds/oxide_os_32.iso"

# cargo clean


# 1. Build kernel ELF
echo "[*] Building kernel..."
echo cargo build --target targets/$TARGET -Zbuild-std=core,alloc
cargo build --target targets/$TARGET -Zbuild-std=core,alloc 2>&1 | tee build.log
