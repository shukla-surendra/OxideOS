# OxideOS — Operating System in Rust

OxideOS is a hobby operating system written in Rust (`no_std`, x86_64) using the Limine bootloader.

![OxideOS Screenshot](./oxideos.png)

---

## Features

| Subsystem | Details |
|-----------|---------|
| Boot | Limine v9, BIOS + UEFI, GDT/TSS/IDT |
| Scheduler | Preemptive round-robin, 8 tasks, Ring 3, fork/exec/waitpid/exit |
| Memory | Paging allocator, per-process CR3, brk/sbrk heap |
| Syscalls | Linux x86-64 ABI (`int 0x80` + `SYSCALL/SYSRET`), OxideOS extensions ≥400 |
| Storage | ATA PIO, FAT16 r/w, ext2 read-only, MBR partition table |
| GUI | Double-buffered framebuffer, window manager, compositor IPC, start menu |
| Network | RTL8139 NIC, smoltcp (TCP/UDP/ICMP/DHCP/ARP) |
| Shell | `/bin/sh` with pipes, env vars, argv/argc |
| Userspace | 30+ programs: ls, cat, ps, cp, grep, wc, wget, edit, nc, filemanager, … |
| **Installer** | `/bin/install` — writes OxideOS to a blank second disk from inside the OS |

---

## Quick Start (QEMU)

```bash
# 1. Install dependencies (Ubuntu/Debian)
sudo apt install build-essential qemu-system-x86 xorriso mtools dosfstools e2fsprogs

# 2. Install Rust nightly
curl https://sh.rustup.rs -sSf | sh
rustup override set nightly

# 3. Build and run
make run-bios          # BIOS boot with ATA disk support (recommended for dev)
make run-gui-x86_64    # UEFI boot with SDL display (GUI + mouse)
```

---

## Developer Testing Workflow

This section covers the fastest loop for building and testing OxideOS while developing.

### Dependencies

| Tool | Purpose | Install |
|------|---------|---------|
| `qemu-system-x86_64` | Run the OS | `apt install qemu-system-x86` |
| `xorriso` | Build the boot ISO | `apt install xorriso` |
| `mtools` (`mformat`, `mcopy`) | Create FAT disk images | `apt install mtools` |
| `dosfstools` (`mkfs.fat`) | Alternative FAT formatter | `apt install dosfstools` |
| `e2fsprogs` (`mke2fs`) | Create ext2 disk | `apt install e2fsprogs` |
| `sfdisk` | Partition the install image | `apt install util-linux` |
| Rust nightly | Compile kernel + userspace | `rustup override set nightly` |
| `nasm` | Assemble userspace `.asm` programs | `apt install nasm` |
| `gcc` cross (`x86_64-linux-gnu-gcc`) | Compile C userspace programs | `apt install gcc-x86-64-linux-gnu` |

### Build Commands

```bash
make                   # build ISO (oxide_os-x86_64.iso)
make kernel            # rebuild kernel only (skips userspace if unchanged)
make userspace         # rebuild userspace only
make clean             # remove ISO and build artefacts
make distclean         # remove everything including limine/ and ovmf/
```

### QEMU Run Targets

| Target | Machine | Display | Disk | Notes |
|--------|---------|---------|------|-------|
| `make run-bios` | `-M pc` (i440FX) | stdio serial | FAT16 on ATA | **Best for dev** — ATA works, fast boot |
| `make run-gui-x86_64` | q35 + UEFI | SDL window | FAT16 on ATA | Full GUI + mouse; grab with first click, release Ctrl+Alt+G |
| `make run-x86_64` | q35 + UEFI | stdio serial | none | Headless UEFI boot |
| `make run-kvm-x86_64` | q35 + KVM | GTK | FAT16 | Hardware-accelerated (WSL2: enable nested virt) |
| `make run-install-x86_64` | q35 + UEFI | SDL | install image | Test the pre-built install image |
| `make run-install-bios` | `-M pc` | stdio | install image | BIOS-boot the install image |

### Persistent FAT16 Data Disk

The kernel mounts a FAT16 disk at `/disk/`. Create it once and it persists across boots:

```bash
make disk              # creates oxide_disk.img (4 MB FAT16, only needed once)
make run-bios          # ATA disk auto-detected on this target
```

Files placed in `oxide_disk.img` are visible to the kernel as `/disk/<name>`.  
To populate it from the host:

```bash
# Mount on Linux
sudo mount -o loop oxide_disk.img /mnt
sudo cp myfile.txt /mnt/
sudo umount /mnt
```

### Secondary ext2 Disk (optional)

```bash
make ext2-disk         # creates oxide_ext2.img (32 MB ext2)
# Populate:
sudo mount -o loop oxide_ext2.img /mnt && sudo cp files /mnt/ && sudo umount /mnt
make run-bios          # both disks attached automatically if the images exist
```

### Typical Dev Loop

```bash
# Edit kernel source
$EDITOR kernel/src/kernel/fat.rs

# Rebuild and test (fastest path)
make kernel && make run-bios

# After editing userspace
make userspace && make kernel && make run-bios

# Full clean rebuild
make distclean && make run-bios
```

### Testing Installer in QEMU

To test the `/bin/install` workflow in QEMU (installs to a second blank disk):

```bash
# 1. Create a blank target disk (192 MB minimum)
dd if=/dev/zero bs=1M count=256 of=test_target.img

# 2. Boot the ISO with both disks attached
qemu-system-x86_64 \
    -M pc \
    -serial stdio \
    -cdrom oxide_os-x86_64.iso \
    -boot d \
    -drive file=oxide_disk.img,format=raw,if=ide,index=0 \
    -drive file=test_target.img,format=raw,if=ide,index=1 \
    -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0

# 3. Inside OxideOS shell:
install       # follow the prompts, type YES

# 4. After install completes, quit QEMU (Ctrl+C in terminal)

# 5. Boot from the installed disk (no ISO, no oxide_disk.img)
qemu-system-x86_64 \
    -M q35 \
    -serial stdio \
    -drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
    -drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
    -drive file=test_target.img,format=raw,if=ide,index=0 \
    -display sdl \
    -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0
```

Or use the pre-built install image (no ISO + installer step needed):

```bash
make install-image          # builds oxide_install.img once
make run-install-x86_64     # UEFI boot from the pre-built image
make run-install-bios       # BIOS boot from the pre-built image
```

---

## Installation on Real Hardware / VirtualBox

OxideOS supports two installation methods.

### Method A — Pre-built image (simplest)

Produces a ready-to-boot disk image without running any installer.

```bash
make install-image          # builds oxide_install.img (192 MB)
```

Write to a USB drive:

```bash
sudo ./install.sh /dev/sdX  # DANGER: replaces ALL data on /dev/sdX
```

Boot the USB on any x86_64 machine with UEFI or legacy BIOS firmware.

#### VirtualBox with pre-built VDI

```bash
make install-vdi            # converts oxide_install.img → oxide_install.vdi
```

1. **New VM** → Name: `OxideOS`, Type: `Other`, Version: `Other/Unknown (64-bit)`
2. **Hardware** → RAM: 256 MB minimum
3. **Hard Disk** → *Use an existing virtual hard disk file* → select `oxide_install.vdi`
4. **Settings → System → Motherboard** → Enable EFI for UEFI boot  
   *(or leave disabled for BIOS boot — both work)*
5. **Start**

---

### Method B — Live installer (real OS install flow)

Boot OxideOS from the ISO with a blank second disk attached, then run `/bin/install` inside the OS. This is the traditional "boot from CD, install to disk" workflow.

#### VirtualBox Setup

**Step 1 — Create the VM**

1. Open VirtualBox → **New**
2. Name: `OxideOS`, Type: `Other`, Version: `Other/Unknown (64-bit)`
3. RAM: 256 MB (512 MB recommended)
4. **Hard Disk** → *Create a new virtual hard disk*
   - Format: VDI (or VMDK)
   - Size: **256 MB minimum** (512 MB recommended)
   - This will be the **installation target** — it starts blank

**Step 2 — Attach the boot ISO**

1. **Settings → Storage**
2. Click the CD icon → *Choose a disk file* → select `oxide_os-x86_64.iso`
3. Ensure the controller type is **IDE** (not SATA) for compatibility

**Step 3 — Configure boot order**

1. **Settings → System → Motherboard**
2. Boot order: **Optical** first, then **Hard Disk**
3. Enable EFI if you want UEFI boot (optional — BIOS boot also works)

**Step 4 — Start and install**

1. **Start** the VM — OxideOS boots from the ISO
2. In the terminal window, type:
   ```
   install
   ```
3. The installer shows the target disk size and asks for confirmation:
   ```
   OxideOS Installer  v0.1
   Target disk: 256 MB  (524288 sectors)

   Type YES to continue, anything else to abort: YES

   Installing OxideOS...
     [1/4] Formatting EFI boot partition (FAT32)...
     [2/4] Formatting data partition (FAT16)...
     [3/4] Writing Limine + kernel to boot partition...
     [4/4] Writing MBR partition table...

   Installation complete!
   ```
4. **Shut down** the VM (start menu → Shutdown, or `kill 1` in shell)

**Step 5 — Remove the ISO and boot from disk**

1. **Settings → Storage** → remove the ISO from the optical drive
2. **Settings → System → Motherboard** → move **Hard Disk** to first in boot order
3. **Start** — OxideOS boots from the installed disk

> **Note:** The installed system includes the kernel that was running during install (the binary is captured from Limine at boot time). A clean rebuild and re-install updates the kernel on disk.

#### QEMU Setup (equivalent)

```bash
# 1. Create blank target disk
dd if=/dev/zero bs=1M count=256 of=install_target.img

# 2. Boot ISO with blank disk as second drive
qemu-system-x86_64 \
    -M pc -serial stdio \
    -cdrom oxide_os-x86_64.iso -boot d \
    -drive file=oxide_disk.img,format=raw,if=ide,index=0 \
    -drive file=install_target.img,format=raw,if=ide,index=1 \
    -m 2G -cpu max

# 3. Run 'install' in the OxideOS shell, type YES

# 4. Boot from the installed disk (UEFI)
qemu-system-x86_64 \
    -M q35 -serial stdio \
    -drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
    -drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
    -drive file=install_target.img,format=raw,if=ide,index=0 \
    -display sdl -m 2G -cpu max
```

#### What the installer writes

| Step | Action |
|------|--------|
| 1 | Format partition 1 (64 MB, FAT32) — EFI boot partition |
| 2 | Format partition 2 (64 MB, FAT16) — OxideOS data partition |
| 3 | Write `EFI/BOOT/BOOTX64.EFI` → UEFI bootloader |
| 3 | Write `boot/limine/limine-bios.sys` → BIOS boot stage 2 |
| 3 | Write `boot/limine/limine.conf` → boot configuration |
| 3 | Write `boot/kernel` → OxideOS kernel (with all userspace embedded) |
| 4 | Write MBR (partition table + BIOS bootstrap) — **written last** |

The MBR is written last: if anything fails mid-install, the disk has no valid MBR and will not attempt to boot, which is a safe failure state.

---

## Installed Disk Layout

```
oxide_install.img  (192 MB)
├── MBR  [LBA 0]
│   ├── Limine BIOS bootstrap (bytes 0–439)
│   ├── Partition 1: EFI  (type 0xEF, LBA 2048–133119,  64 MB)
│   └── Partition 2: Data (type 0x06, LBA 133120–264191, 64 MB)
│
├── Partition 1 — FAT32 (EFI System)
│   ├── EFI/BOOT/BOOTX64.EFI     ← UEFI entry point (Limine)
│   ├── boot/limine/limine.conf  ← boot config
│   ├── boot/limine/limine-bios.sys
│   └── boot/kernel              ← OxideOS kernel binary
│
└── Partition 2 — FAT16 (OxideOS data)
    └── (empty — user files go here, mounted as /disk/)
```

---

## Makefile Reference

| Target | Description |
|--------|-------------|
| `make` | Build ISO (`oxide_os-x86_64.iso`) |
| `make kernel` | Rebuild kernel only |
| `make userspace` | Rebuild userspace only |
| `make run-bios` | Boot ISO in QEMU (BIOS, ATA disk, serial output) |
| `make run-gui-x86_64` | Boot ISO in QEMU (UEFI, SDL window, mouse) |
| `make run-x86_64` | Boot ISO in QEMU (UEFI, serial only) |
| `make run-kvm-x86_64` | Boot ISO in QEMU with KVM acceleration |
| `make disk` | Create `oxide_disk.img` — 4 MB FAT16 data disk (once) |
| `make ext2-disk` | Create `oxide_ext2.img` — 32 MB ext2 secondary disk (once) |
| `make install-image` | Build `oxide_install.img` — 192 MB pre-installed bootable disk |
| `make install-vdi` | Convert install image to VirtualBox VDI format |
| `make run-install-x86_64` | Boot `oxide_install.img` in QEMU (UEFI, SDL) |
| `make run-install-bios` | Boot `oxide_install.img` in QEMU (BIOS) |
| `make clean` | Remove ISO and build artefacts |
| `make clean-disk` | Remove `oxide_disk.img` |
| `make clean-install` | Remove `oxide_install.img` and `oxide_install.vdi` |
| `make distclean` | Remove all build output including `limine/` and `ovmf/` |

---

## ATA Disk and QEMU Machine Types

OxideOS uses ATA PIO for disk I/O (port `0x1F0`). This requires the legacy IDE controller:

| QEMU flag | Chipset | IDE at 0x1F0? | Use case |
|-----------|---------|--------------|----------|
| `-M pc` | i440FX/PIIX4 | ✓ Yes (legacy mode) | Dev testing, BIOS boot |
| `-M q35` + UEFI | ICH9 | ✗ No (native PCI) | GUI testing (disk not needed for ISO boot) |
| `-M q35` + install image as IDE | ICH9 | ✓ Yes (IDE HDD) | Installed disk boot |

The installed disk attached as `-drive if=ide,index=0` works with q35 because the kernel detects the drive via ATA — q35's ICH9 does expose IDE in legacy mode when not overridden by UEFI.

---

## WSL2 Notes

- SDL display requires an X server on Windows (e.g., VcXsrv, WSLg, or X410).
- KVM acceleration requires nested virtualisation: add `nestedVirtualization=true` to `~/.wslconfig` and restart WSL.
- All other targets work without X server (serial-only output via `stdio`).

---

## License

See [LICENSE](./LICENSE).
