# Oxide OS – Hobby Operating System in Rust

Oxide OS is a hobby operating system written in Rust. This repository demonstrates how to set up a basic kernel using Rust and the Limine bootloader.

## Overview

This project is intended for learning and experimentation with OS development in Rust. You are free to use, modify, and improve Oxide OS. For full licensing terms, see the [LICENSE](./LICENSE) file.

## How to Use

### Dependencies

- The `make` commands depend on **GNU Make (`gmake`)**. On most GNU/Linux distributions, `make` will work; on non-GNU systems, `gmake` may be required.  
- All `make all*` targets require **Rust** to be installed.  
- Building a bootable ISO (`make all`) requires **xorriso**.  
- Building a HDD/USB image (`make all-hdd`) requires **sgdisk** (from `gdisk` or `gptfdisk`) and **mtools**.

### Architectural Targets

The `KARCH` make variable specifies the target architecture for the kernel and image.  

- Default: `x86_64`  
- Other options: `aarch64`, `riscv64`, `loongarch64`  

Additional architectures may need to be enabled in `kernel/rust-toolchain.toml`.

### Makefile Targets

| Target | Description |
|--------|-------------|
| `make all` | Compile the kernel and generate a bootable ISO image. |
| `make all-hdd` | Compile the kernel and generate a raw HDD/USB image. |
| `make run` | Build and run in QEMU (UEFI, q35, no disk). |
| `make run-gui` | Same as `run` but with SDL display for mouse support. |
| `make run-bios` | Build and run in QEMU using **BIOS + `-M pc`** — required for ATA disk access. |
| `make disk` | Create `oxide_disk.img` — a 4 MB FAT16 disk image (run once). |
| `make clean` | Remove build artefacts and ISO. |
| `make clean-disk` | Remove `oxide_disk.img`. |

### Booting with a Persistent Disk

ATA PIO disk access requires the legacy IDE controller at I/O port `0x1F0`, which is only
available with the i440FX/PIIX4 chipset (`-M pc`). The default `run` targets use q35 + UEFI
where the IDE port floats.

```bash
make disk       # create oxide_disk.img (once)
make run-bios   # boot with disk attached — shows "ATA detected" in System Info
```

See [docs/disk_and_filesystem.md](docs/disk_and_filesystem.md) for the full explanation.

## License

See the [LICENSE](./LICENSE) file for licensing details.
