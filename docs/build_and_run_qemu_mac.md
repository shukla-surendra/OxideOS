```
rustup override set nightly
```
```
brew install qemu
```

```
qemu-system-x86_64 --version
```

```
cargo install bootimage
```

```
rustup component add rust-src --toolchain nightly-aarch64-apple-darwin
```

```
brew install llvm
```
```
brew install lld
```

```
rustup component add llvm-tools-preview
```

```
cargo bootimage --target x86_64-oxideos.json -Zbuild-std=core,alloc
```

```
qemu-system-x86_64 -drive format=raw,file=target/x86_64-oxideos/debug/bootimage-OxideOs.bin
```

# for bin to iso

On Linux: grub-mkrescue automates all this.

On macOS: you only get grub-mkimage → meaning you must manually script what grub-mkrescue would do (core image + boot.img + mkisofs).

Many OS devs just spin up a Linux VM so they can use grub-mkrescue directly and avoid these manual steps.

