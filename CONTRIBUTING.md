# Contributing to OxideOS

OxideOS is a hobby OS written in Rust. Contributions of all kinds are welcome — new syscalls, drivers, filesystem features, userspace programs, documentation fixes, and bug reports.

## Quick start

```bash
# 1. Fork and clone
git clone https://github.com/YOUR_USERNAME/OxideOS
cd OxideOS

# 2. Install dependencies (Ubuntu/Debian)
sudo apt install build-essential qemu-system-x86 xorriso mtools \
                 dosfstools e2fsprogs nasm

# 3. Install Rust nightly
curl https://sh.rustup.rs -sSf | sh
rustup override set nightly

# 4. Build and run
make run-bios
```

## Where to start

Good first issues are labeled [`good first issue`](https://github.com/SurendraShuklaOfficial/OxideOS/issues?q=label%3A%22good+first+issue%22).

Easy areas to contribute:
- **New syscall stub** — add a Linux-compatible syscall that returns a sensible value. Pattern is in `kernel/src/kernel/syscall_core.rs` + `syscall.rs`.
- **New coreutil** — add a program to `userspace/coreutils/src/`. Pattern: see `wc.rs` or `head.rs`.
- **Bug fix** — pick any open bug issue and reproduce it in QEMU first.
- **Documentation** — improve `docs/plan.md` or add inline comments to tricky kernel code.

## Project layout

```
kernel/src/kernel/
├── main.rs              # entry point, subsystem init
├── scheduler.rs         # preemptive round-robin scheduler, Task struct
├── syscall_core.rs      # syscall enum, dispatch, trait stubs
├── syscall.rs           # KernelRuntime impl — concrete syscall implementations
├── vfs.rs               # virtual filesystem layer
├── fat.rs               # FAT16 r/w driver
├── ext2.rs              # ext2 read-only driver
├── fs/ramfs.rs          # in-memory RamFS + FdTable
├── paging_allocator.rs  # physical frame allocator, page tables
├── programs.rs          # embedded userspace binaries
└── net/                 # RTL8139 + smoltcp TCP/IP stack

userspace/
├── oxide-rt/            # no_std Rust runtime (syscall wrappers)
├── sh/                  # /bin/sh shell
├── coreutils/           # ls, cat, grep, wc, head, tail, sort, …
├── terminal/            # GUI terminal emulator
├── wget/ nc/ edit/      # network and editor programs
└── hello_musl/          # C programs via musl libc (reference)
```

## Adding a syscall

1. Add the variant to the `Syscall` enum in `syscall_core.rs` with the Linux number.
2. Add the `name()` entry and `From<u64>` mapping.
3. Add a trait method stub (default → `ENOSYS` or a safe value).
4. Add a dispatch arm in `dispatch()`.
5. Override the trait method in `syscall.rs` with the real implementation.
6. Test with a musl-compiled C program or by checking BusyBox behaviour.

## Adding a userspace program

1. Create a new binary crate under `userspace/` or add a file to `coreutils/src/`.
2. Use `oxide-rt` for syscalls — see `userspace/oxide-rt/src/lib.rs`.
3. Add it to the `Cargo.toml` workspace and to `userspace/Makefile`.
4. Embed it in `kernel/src/kernel/programs.rs` (`include_bytes!`).

## Code style

- No `std` in the kernel — `no_std` only.
- Unsafe is allowed where necessary; mark invariants with a short comment.
- No external kernel crates beyond what's already in `Cargo.toml`.
- Keep commits small and focused. One feature / fix per PR.

## Testing

Always test with at least:

```bash
make run-bios   # boots, shell works, no panic
```

For syscall changes also verify:
```bash
# Inside QEMU shell:
hello_musl      # basic musl libc smoke test
lua             # Lua REPL starts
busybox ls /    # BusyBox directory listing
```

## Reporting bugs

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.yml). Include the QEMU serial output and the exact commit hash.
