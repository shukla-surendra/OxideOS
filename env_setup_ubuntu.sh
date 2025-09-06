sudo apt update
sudo apt  install rustc
sudo apt  install rustup
rustup override set nightly
sudo apt update
sudo apt install qemu-system
qemu-system-x86_64 --version
qemu-system-i386 --version
sudo bash -c "$(wget -O - https://apt.llvm.org/llvm.sh)"
sudo ln -s /usr/bin/lld-20 /usr/bin/lld
rustup component add llvm-tools-preview
sudo apt update
sudo apt install build-essential
rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu
sudo apt update
sudo apt install grub2-common grub-pc-bin xorriso