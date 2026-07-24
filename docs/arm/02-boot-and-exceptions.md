# 02 — aarch64 boot, exception vectors, serial console

Status: **done** (boots to serial + framebuffer test pattern on QEMU virt).

## What was built

- **`arch/aarch64/serial.rs`** — PL011 UART driver (QEMU virt UART0, phys
  `0x0900_0000`), API-compatible with the x86 16550 driver so shared code
  (panic handler, loggers) uses `crate::kernel::serial::SERIAL_PORT`
  unchanged. Registers are addressed through the Limine HHDM after `init()`.
- **`arch/aarch64/exceptions.rs`** — EL1 vector table (16 slots, 2 KiB
  aligned) installed via `VBAR_EL1`. Every exception dumps ESR/ELR/FAR to
  serial and parks the CPU; IRQ dispatch arrives with the GIC step.
- **aarch64 `kmain`** in `main.rs` — serial console → vector install → HHDM
  and framebuffer report → gradient test pattern → `wfi` park.
- **cfg-gating** — `kernel/mod.rs` and `main.rs` compile the x86-only
  subsystems (drivers, mem, fs, proc, ipc, sys, gui, installer, desktop)
  only on x86_64; aarch64 re-enables them one port step at a time. A
  placeholder `#[global_allocator]` that fails all allocations keeps the
  aarch64 image linking until the memory step brings a real heap.

## Decisions and gotchas (read before the next steps)

1. **Limine base revision is pinned to 2 on aarch64** (`main.rs`). Revision 3
   made the HHDM restrictive (RAM + framebuffer only), which faults on PL011
   MMIO. Revision ≤2 keeps the unconditional direct map of the first 4 GiB.
   The memory step (04) should map device memory explicitly (Device-nGnRE
   attributes) and move back to revision 3. x86_64 stays on revision 3.
2. **Limine enters the kernel with `SPSel = 0`** (running on SP_EL0), and
   SP_EL1 is garbage. Exceptions taken from SP_EL0 pivot to SP_EL1, so
   `aarch64_install_vectors` points SP_EL1 at a dedicated 16 KiB exception
   stack in `.bss` first — otherwise the handler prologue itself data-aborts
   (observed during bring-up as a nested abort with FAR just past the image).
3. The exception handlers never return, so the vector stubs save no frame yet.
   The GIC/timer step must add a real save/restore frame before IRQs return.
4. **Never run aarch64 QEMU with `-cpu max`** — Limine v9's higher-half
   handoff fails on that CPU model (`TTBR1_EL1` left null, so the jump to the
   kernel entry instruction-aborts into a recursive fault loop before any
   kernel code runs; symptom: silence after `BdsDxe: starting Boot0002`).
   The Makefile's default `QEMUFLAGS` is arch-conditional for this reason;
   the run targets pin `-cpu cortex-a72`. Also note: commas inside
   `$(call USER_VARIABLE,...)` values must be written as `$(comma)` or make
   silently drops everything after them.

## Verified

- `readelf`: AArch64 ELF64, higher-half entry; boots via Limine `BOOTAA64.EFI`.
- Serial log shows: EL1 confirmed, vectors installed, HHDM at
  `0xFFFF000000000000`, 1024×768 framebuffer painted.
- Deliberate `read_volatile(0xDEAD_BEEF_000)` produced a correct dump:
  `Sync (EL1, SP_EL0)`, ESR `0x96000004` (data abort, translation fault),
  FAR `0x00000DEADBEEF000`.
- x86_64 build unaffected (both targets compile from the same tree).

## Run it

```sh
make KARCH=aarch64 run        # SDL window + serial on stdout
make KARCH=aarch64 run MODE=uefi QEMUFLAGS="-display none"   # headless serial
```
