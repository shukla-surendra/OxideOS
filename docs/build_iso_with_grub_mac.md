# Build a Bootable ISO with GRUB on macOS (BIOS + UEFI)

> This guide shows how to package this OS kernel into a bootable **.iso** on macOS using **GRUB 2**.  
> It covers prerequisites, folder layout, a minimal `grub.cfg`, building the ISO, testing in QEMU, and fixing common errors.

---

## 0) Kernel Requirements

- Target: **x86_64**
- Format: **ELF** file (e.g., `kernel.elf`) recommended
- Boot protocol: **Multiboot2** (or Multiboot v1)  

Verify with:

```bash
grub-file --is-x86-multiboot2 path/to/kernel.elf && echo "OK: multiboot2"
# or:
grub-file --is-x86-multiboot path/to/kernel.elf && echo "OK: multiboot v1"
```

If you see `no multiboot header found`, binary doesn’t have a proper Multiboot header. Fix that before proceeding.

---

## 1) Install Prerequisites (Homebrew)

```bash
# Install Homebrew if missing:
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Tools
brew install grub xorriso mtools qemu edk2-ovmf coreutils
```

- `grub-mkrescue` → builds the ISO  
- `xorriso`, `mtools` → required for UEFI boot entries  
- `edk2-ovmf` → provides UEFI firmware for QEMU  

---

## 2) Project Layout

```bash
mkdir -p build/iso/boot/grub
cp path/to/kernel.elf build/iso/boot/kernel.elf
```

Result:

```
build/
  iso/
    boot/
      grub/
        grub.cfg
      kernel.elf
```

---

## 3) Minimal `grub.cfg`

`build/iso/boot/grub/grub.cfg`:

```cfg
set timeout=0
set default=0

menuentry "My OS (Multiboot2)" {
    multiboot2 /boot/kernel.elf
    boot
}

# For Multiboot v1 kernels, replace with:
# menuentry "My OS (Multiboot v1)" {
#     multiboot /boot/kernel.elf
#     boot
# }
```

---

## 4) Build the ISO

### BIOS-only ISO (simple)
```bash
grub-mkrescue -o dist/myos-bios.iso build/iso
```

### Hybrid BIOS + UEFI ISO (recommended)
```bash
grub-mkrescue -o dist/myos.iso build/iso
```

If you get “command not found”, try:

```bash
$(brew --prefix)/opt/grub/bin/grub-mkrescue -o dist/myos.iso build/iso
```

Check EFI entry exists:

```bash
xorriso -indev dist/myos.iso -report_el_torito as_mkisofs | grep -i efi || true
```

---

## 5) Test in QEMU

### BIOS boot
```bash
qemu-system-x86_64 -m 512 -cdrom dist/myos.iso
```

### UEFI boot
```bash
OVMF_CODE="$(brew --prefix)/share/edk2-ovmf/x64/OVMF_CODE.fd"

qemu-system-x86_64 \
  -m 512 \
  -bios "$OVMF_CODE" \
  -cdrom dist/myos.iso
```

---

## 6) Common Issues

**A) `no multiboot header found`**  
- Kernel is not Multiboot-compliant  
- Ensure proper header + linker script  
- Use `multiboot2` directive in `grub.cfg` if header is v2  

**B) `grub-mkrescue: command not found`**  
- Use full path:  
  ```bash
  $(brew --prefix)/opt/grub/bin/grub-mkrescue
  ```  
- Reinstall: `brew reinstall grub xorriso mtools`

**C) UEFI fails but BIOS works**  
- Ensure `mtools` and `xorriso` installed before running `grub-mkrescue`  
- Confirm EFI entry using `xorriso -report_el_torito`  

**D) Only GRUB prompt shows up**  
- Wrong or missing `grub.cfg`  
- Must be at `/boot/grub/grub.cfg` inside ISO  

**E) Using `.bin` instead of `.elf`**  
- Flat binaries work only if they include a proper Multiboot header  
- ELF is preferred  

---

## 7) Optional Build Script

`scripts/mkiso.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO_DIR="$ROOT/build/iso"
DIST_DIR="$ROOT/dist"

mkdir -p "$ISO_DIR/boot/grub" "$DIST_DIR"

# Copy kernel
cp "$ROOT/path/to/kernel.elf" "$ISO_DIR/boot/kernel.elf"

# Create grub.cfg if missing
if [[ ! -f "$ISO_DIR/boot/grub/grub.cfg" ]]; then
  cat > "$ISO_DIR/boot/grub/grub.cfg" <<'CFG'
set timeout=0
set default=0
menuentry "My OS (Multiboot2)" {
    multiboot2 /boot/kernel.elf
    boot
}
CFG
fi

GRUB_MKRESCUE="$(brew --prefix)/opt/grub/bin/grub-mkrescue"
"$GRUB_MKRESCUE" -o "$DIST_DIR/myos.iso" "$ISO_DIR"

echo "Built: $DIST_DIR/myos.iso"
```

Make executable:

```bash
chmod +x scripts/mkiso.sh
scripts/mkiso.sh
```
