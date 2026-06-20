# Boot — Design Notes

For the step-by-step walkthrough of what actually happens at boot, see
[oxide_cocepts/01_boot_process.md](oxide_cocepts/01_boot_process.md). This
doc covers the *why* behind the choices, not the *how*.

## Bootloader: Limine, not a hand-rolled loader or GRUB

OxideOS uses the [Limine](https://github.com/limine-bootloader/limine)
protocol rather than writing a custom bootloader or targeting Multiboot2/GRUB.

- A hand-rolled bootloader (real-mode stage → protected mode → long mode)
  is a well-understood but large amount of code that has nothing to do with
  the kernel itself — Limine already solves it correctly for both BIOS and
  UEFI.
- Limine hands the kernel a clean 64-bit long-mode environment, a memory
  map, and a linear framebuffer via a typed request/response protocol
  (`limine` crate), instead of the kernel having to parse Multiboot2 tag
  soup or do its own E820/UEFI memory-map walking.
- One kernel ELF boots identically under BIOS and UEFI — Limine absorbs
  that difference entirely; `kmain()` never branches on which firmware
  booted it.

The repo briefly carried GRUB-based build docs from an earlier
experiment; they were removed once the project committed to Limine
exclusively (see git history / `docs/plan.md`).

## Higher-half kernel

The kernel is mapped at `0xFFFF800000000000` (Limine's HHDM base) rather
than running identity-mapped at low addresses. This keeps the kernel's
address range fixed and disjoint from every process's user-space layout,
so kernel pointers are never accidentally valid (or aliased) as user
addresses, and the same kernel page-table entries (L4 indices 256–511) can
simply be copied into every new process's page table instead of being
re-derived per process.

## Current limitations

- x86_64 only in practice; aarch64/riscv64 targets exist in the Makefile
  but the interrupt/paging code is x86_64-specific.
- No initrd/ramdisk handoff from the bootloader — userspace binaries are
  `include_bytes!`-embedded into the kernel image itself
  (`kernel/src/kernel/proc/programs.rs`), which keeps boot simple but means
  every userspace change requires a kernel rebuild.
- Single Limine revision is asserted (`BASE_REVISION.is_supported()`); no
  fallback path if a future Limine major version changes the protocol.
