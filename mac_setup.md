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