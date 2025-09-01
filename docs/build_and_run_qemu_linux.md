```
rustup override set nightly
```
```
sudo apt-get update
sudo apt-get install qemu-system
```

```
qemu-system-x86_64 --version
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


```
rustup component add llvm-tools-preview
```

install build essential

```
sudo apt update
sudo apt install build-essential

```
Install Bootimage

```
cargo install bootimage
```

```
cargo bootimage --target x86_64-oxideos.json -Zbuild-std=core,alloc
```

```
rustup component add rust-src --toolchain nightly-aarch64-unknown-linux-gnu
```

```
qemu-system-x86_64 -drive format=raw,file=target/x86_64-oxideos/debug/bootimage-OxideOs.bin
```

# for bin to iso

On Linux: grub-mkrescue automates all this.

On macOS: you only get grub-mkimage → meaning you must manually script what grub-mkrescue would do (core image + boot.img + mkisofs).

Many OS devs just spin up a Linux VM so they can use grub-mkrescue directly and avoid these manual steps.

