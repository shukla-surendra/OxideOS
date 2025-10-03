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

- `make all` – Compile the kernel (`kernel/`) and generate a bootable ISO image.  
- `make all-hdd` – Compile the kernel and generate a raw image suitable for USB stick or HDD/SSD.  
- `make run` – Build the kernel and bootable ISO, then run it in QEMU (if installed).  
- `make run-hdd` – Build the kernel and raw HDD image, then run in QEMU.  
- `run-uefi` / `run-hdd-uefi` – Equivalent to above targets but boot QEMU with UEFI-compatible firmware.  

## License

See the [LICENSE](./LICENSE) file for licensing details.
