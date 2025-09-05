#!/usr/bin/env bash
# set -e  # exit on first error

TARGET="x86_32-oxideos.json"
KERNEL_NAME="OxideOs"
BUILD_DIR="os_iso_build"
ISO_NAME="OxideOS.iso"

cargo clean
rm -rf os_iso_build
rm OxideOS.iso

# 1. Build kernel ELF
echo "[*] Building kernel..."
cargo build --target $TARGET -Zbuild-std=core,alloc

# 2. Prepare ISO directory
echo "[*] Setting up ISO directory structure..."
rm -rf $BUILD_DIR
mkdir -p $BUILD_DIR/boot/grub

# 3. Copy kernel
cp target/x86_32-oxideos/debug/$KERNEL_NAME $BUILD_DIR/boot/kernel.elf

# 4. Write grub.cfg
cat > $BUILD_DIR/boot/grub/grub.cfg <<EOF
set timeout=0
set default=0

menuentry "OxideOS" {
    insmod all_video
    insmod gfxterm
    insmod vbe
    insmod vga
    set gfxmode=1024x768x32
    set gfxpayload=keep
    terminal_output gfxterm

    multiboot2 /boot/kernel.elf
    boot
}
EOF

# 5. Build ISO
echo "[*] Creating ISO..."
grub-mkrescue -o $ISO_NAME $BUILD_DIR

echo "[*] Done. ISO available as $ISO_NAME"
echo "Run with: qemu-system-i386 -cdrom $ISO_NAME"
# qemu-system-i386 -cdrom OxideOS.iso -serial stdio
qemu-system-i386 -cdrom OxideOS.iso
