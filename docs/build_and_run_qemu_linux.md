## Installation & Setup
### Install Rust
```
sudo apt update
sudo apt  install rustc
sudo apt  install rustup
```
### Setup Nightly
```
rustup override set nightly
```
### Install Qemu for Testing OS
```
sudo apt update
sudo apt install qemu-system
```
Check if exists
```
qemu-system-x86_64 --version
qemu-system-i386 --version
```
### Install LLVM
use this like to find command for llvm installation

https://apt.llvm.org/

```
sudo bash -c "$(wget -O - https://apt.llvm.org/llvm.sh)"

```

this lld install many packages with version suffix for example lld is installed like
lld-20

make a symlink to fix this

```
sudo ln -s /usr/bin/lld-20 /usr/bin/lld
```
### Install llvm-tools-preview

```
rustup component add llvm-tools-preview
```

### Install build essential

```
sudo apt update
sudo apt install build-essential

```
### Install nightly-x86_64-unknown-linux-gnu Toolchain
```
rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu
```
### Install Grub and Grub Rescue
```
sudo apt update
sudo apt install grub2-common grub-pc-bin xorriso
```
## Compiling and Running Using Bootimage
### Build Kernel (Or See below for Running as ISO using Grub)
```
cargo build --target x86_64-oxideos.json -Zbuild-std=core,alloc
```

### Run Kernel (Or See below for Running as ISO using Grub)
```
qemu-system-i386: -drive format=raw,file=target/x86_32-oxideos/debug/bootimage-OxideOs.bin: Could not open 'target/x86_32-oxideos/debug/bootimage-OxideOs.bin
```

## Loading AS ISO with Grub

On Linux: grub-mkrescue automates all this.

On macOS: you only get grub-mkimage → meaning you must manually script what grub-mkrescue would do (core image + boot.img + mkisofs).

Many OS devs just spin up a Linux VM so they can use grub-mkrescue directly and avoid these manual steps.

```
bash build_iso.sh
```

