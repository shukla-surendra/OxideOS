# OxideOS ARM (AArch64) Port

OxideOS is being ported to 64-bit ARM (ARMv8-A, `aarch64`), feature by
feature, alongside the existing x86-64 implementation.  Both architectures
build from the same tree: portable code is shared, architecture-specific
code lives under `kernel/src/kernel/arch/<arch>/`.

Reference platform: **QEMU `virt` machine** (Cortex-A72, GICv2, PL011 UART,
ramfb framebuffer), booted via **Limine v9** on AAVMF (ARM UEFI firmware) —
the same bootloader and boot protocol as the x86-64 build, so the kernel
receives an identical environment on both architectures (higher-half
mapping, HHDM, memory map, framebuffer).

## Feature status

| # | Feature | x86-64 | aarch64 | Doc |
|---|---------|--------|---------|-----|
| 1 | Arch abstraction layer (`arch::cpu` facade, per-arch modules) | ✅ | ✅ | [01-arch-abstraction.md](01-arch-abstraction.md) |
| 2 | Boot: entry, exception vectors, serial console | ✅ GDT/IDT + COM1 | ✅ EL1 vectors + PL011 | [02-boot-and-exceptions.md](02-boot-and-exceptions.md) |
| 3 | Interrupt controller + timer + power | ✅ PIC + PIT + ACPI ports | 🔜 GICv2 + generic timer + PSCI | 04-gic-timer-psci.md |
| 4 | Memory: frame allocator, heap, paging | ✅ 4-level x86 paging | 🔜 4 KB granule, TTBR0/1 | 04-memory.md |
| 5 | Framebuffer GUI desktop | ✅ | 🔜 | 05-gui.md |
| 6 | Scheduler context switch | ✅ | ⏳ planned | — |
| 7 | User mode (EL0) + syscalls (SVC, Linux aarch64 ABI) | ✅ SYSCALL/SYSRET | ⏳ planned | — |
| 8 | Disk: block driver + FAT/ext2 | ✅ ATA PIO | ⏳ planned (virtio-blk) | — |
| 9 | Input: keyboard + mouse | ✅ PS/2 | ✅ virtio-input (polled virtio-mmio) | 03-virtio-input.md |
| 10 | Networking | ✅ RTL8139/e1000/PCnet | ⏳ planned (virtio-net) | — |
| 11 | Userspace programs (aarch64 builds) | ✅ | ⏳ planned | — |

✅ done · 🔜 next up · ⏳ planned

## Why the device story differs

The QEMU `virt` machine (and real ARM boards) has none of the legacy PC
hardware the x86-64 build drives.  Every x86 device needs an ARM-world
counterpart:

| Role | x86-64 (PC) | aarch64 (virt) |
|------|-------------|----------------|
| Serial console | 16550 UART at I/O port `0x3F8` | PL011 MMIO at `0x0900_0000` |
| Interrupt controller | 8259 PIC (port I/O) | GICv2 (MMIO distributor + CPU interface) |
| Timer tick | 8253/8254 PIT, IRQ 0 | ARM generic timer (`CNTP_*` system registers), PPI 30 |
| Power off / reboot | ACPI PM / keyboard-controller reset ports | PSCI (`SYSTEM_OFF` / `SYSTEM_RESET` via SMC/HVC) |
| Disk | ATA PIO (ports `0x1F0`…) | virtio-blk (MMIO/PCI) |
| Keyboard/mouse | 8042 PS/2 (ports `0x60`/`0x64`) | virtio-input (virtio-mmio at `0x0a00_0000`, polled) |
| NIC | RTL8139 / e1000 / PCnet (PCI port I/O) | virtio-net |
| Discovery | PCI config ports `0xCF8`/`0xCFC`, ACPI | Device tree / ECAM PCIe, ACPI (AAVMF) |

There is **no port I/O on ARM at all** — every device is memory-mapped.
This is why the port starts with an abstraction layer rather than `#ifdef`s
scattered through drivers.

## Building and running

```bash
# x86-64 (unchanged)
make KARCH=x86_64        # ISO
make run                 # QEMU with GUI

# aarch64
make KARCH=aarch64       # ISO (BOOTAA64.EFI + kernel)
make KARCH=aarch64 run   # qemu-system-aarch64 -M virt, AAVMF firmware
```

The Rust toolchain file already carries the `aarch64-unknown-none` target;
`kernel/linker-aarch64.ld` and the Makefile `KARCH` plumbing existed from
the Limine template and are used as-is.

## Porting rules

1. **No raw `asm!` outside `arch/<target>/`** (and arch-specific drivers).
   Portable code uses the `arch::cpu` facade (see doc 01).
2. **Same boot protocol on both arches.** Limine gives us the memory map,
   HHDM offset and framebuffer identically; code that only consumes Limine
   responses is already portable.
3. **x86-64 must stay green.** Every ARM feature lands only after
   `make KARCH=x86_64` still builds and boots.
4. **One feature = one commit**, each with its design doc in this
   directory.
