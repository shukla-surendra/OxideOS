# 07 — Drivers: Writing New Hardware Code

A driver is code that translates between hardware's language (registers, port I/O,
interrupts) and the kernel's language (functions, buffers, events). This doc teaches
you how to read a hardware spec and turn it into driver code — using your existing
drivers as models.

> **Architecture scope:** Models A–C below are x86-64 (port I/O and PCI
> don't exist on ARM at all). **Model D** is aarch64 — and unlike docs
> 02/05/06, this one isn't a "not built yet" gap: the aarch64 port's two
> newest drivers, `virtio_blk.rs` and `virtio_input.rs`, are real, working
> code you can read today. See `docs/arm/03-virtio-input.md` and
> `docs/arm/04-virtio-blk.md` for the full write-ups.

---

## How hardware is accessed: two families

x86 has both of the mechanisms below. **ARM (and QEMU's `virt` machine)
has only the second one** — there is no port-mapped I/O on ARM at all,
which is *the* defining fact that shapes every aarch64 driver in this
kernel (see `docs/arm/README.md`'s "Why the device story differs" table).

### Port-mapped I/O (PMIO)
x86 has a separate 64K I/O address space accessed with special instructions:
```asm
out dx, al    ; write byte in AL to port DX
in al, dx     ; read byte from port DX into AL
```
In Rust (inline asm):
```rust
unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value) }
unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value) }
```

Used by: PIC (0x20/0x21), PIT timer (0x40–0x43), PS/2 keyboard (0x60/0x64),
RTC/CMOS (0x70/0x71), PC speaker (0x61), serial UART (0x3F8–0x3FF).

### Memory-mapped I/O (MMIO)
Some devices expose their registers as specific physical memory addresses.
Just read/write those addresses with normal loads/stores — but the CPU must not
cache them (`volatile` reads/writes).

Used by: PCI BARs, framebuffer (the screen), network cards.

---

## Model A: Port-mapped driver — `kernel/src/kernel/drivers/rtc.rs`

The RTC (Real-Time Clock) is a simple device in CMOS. You access it via two ports:
- `0x70` — index register (select which CMOS register to read)
- `0x71` — data register (read/write selected register)

**Read `rtc.rs`:**
- Find the port constants (0x70 / 0x71)
- Find how it reads hours: write the register index to 0x70, then read 0x71
- Find the BCD conversion — hardware may store time as BCD (0x12 = 12, not 18)

**Key pattern:** index + data port pair. This is used by many chips (PIC uses it too).

---

## Model B: Interrupt-driven driver — `kernel/src/kernel/drivers/keyboard.rs`

The PS/2 keyboard doesn't need polling — it fires an interrupt when a key is ready.

**The pattern:**
1. Hardware puts data in register
2. Fires interrupt
3. ISR reads the data (must happen quickly, before the device drops it)
4. Decodes and queues the event
5. Returns from interrupt

**Key constraint:** ISRs must be fast. You cannot allocate memory, take locks,
or do I/O in an ISR. That's why keyboard.rs queues the raw scancode and decodes
it later (or uses a minimal state machine).

---

## Model C: PCI device — `kernel/src/kernel/drivers/net/pci.rs`

PCI devices are discovered by scanning a configuration space:
- Each device has a `(bus, device, function)` address
- Reading config space address `0x00` gives `(vendor_id, device_id)`
- If `vendor_id == 0xFFFF`, no device present
- Otherwise, you can read class code, BAR addresses, interrupt line, etc.

**Read `pci.rs`:**
- Find `pci_read_u32(bus, device, function, offset)`
- Find the scan loop (enumerate bus 0, all 32 device slots, function 0)
- Find how RTL8139 is identified (vendor 0x10EC, device 0x8139)

---

## Model D: MMIO virtio driver (aarch64) — `arch/aarch64/virtio_input.rs`, `virtio_blk.rs`

QEMU's `virt` machine exposes a bank of 32 **virtio-mmio transport slots**
starting at physical address `0x0a00_0000`, each 0x200 bytes apart. Every
virtio device (keyboard, mouse, block device, ...) that QEMU is configured
with lands in one of these slots. There's no bus enumeration step like
PCI's — you just probe each slot in turn and read its magic number and
device-ID register to find out what's there:

```rust
// virtio_input.rs — register offsets are just constants, no chip-specific
// instruction needed to reach them (contrast Model A's port 0x70/0x71 pair)
const MMIO_BASE:   u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS:  u64 = 32;

const REG_MAGIC:     u64 = 0x000; // "virt" = 0x74726976
const REG_DEVICE_ID: u64 = 0x008; // 18 = input, 2 = block device
```

**Reading/writing a register is a plain pointer dereference** — no `in`/`out`
instruction exists on ARM, so "access this device register" *is* "read
this memory address," except the compiler must be stopped from doing what
it would normally do to a plain memory read (caching the value, reordering
it relative to other reads, or optimizing a "redundant" read away
entirely) since the *hardware*, not your program, can change what's there
between two reads of the same address:

```rust
fn mmio_read32(base: u64, off: u64) -> u32 {
    unsafe { ((base + off) as *const u32).read_volatile() }
}
fn mmio_write32(base: u64, off: u64, val: u32) {
    unsafe { ((base + off) as *mut u32).write_volatile(val) }
}
```
(`virtio_input.rs:118-122`)

`read_volatile`/`write_volatile` are the Rust spelling of C's `volatile`
qualifier — see doc 07's original Model A/B/C intro for the same idea
applied to framebuffer/PCI MMIO on the x86 side. Every access here is
`unsafe` for the same reason raw pointer dereferences always are (doc 00
§7): the compiler can't verify `base + off` is actually a valid, mapped
address — that's on the driver author to get right from the datasheet
(here, the [VirtIO 1.1 spec](https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.html)).

**No interrupts, by design (for now).** A real virtio driver would set up
a used-ring and let the device *interrupt* the CPU when data arrives —
but that requires the GICv2 interrupt controller, which isn't wired up on
this port yet (doc 02's ARM note). Instead, both `virtio_blk.rs` and
`virtio_input.rs` are **polled**: the GUI loop calls into them once per
frame to drain whatever's arrived in the used ring since the last check.
This is architecturally the same trade-off C's `keyboard.rs` avoids by
being interrupt-driven (Model B above) — polling burns a little CPU
every frame checking "is anything new," rather than costing nothing until
hardware says so — accepted here as a deliberate bring-up simplification,
not a design endorsement; `docs/arm/03-virtio-input.md` says so explicitly.

**Rust patterns specific to this model:**
- Every MMIO helper function is a **thin, safe-looking wrapper around an
  unsafe operation** — `mmio_read32` itself has no `unsafe` in its
  signature, but its *body* is one `unsafe` block, which is the same
  "small unsafe core, safe(r) surface" pattern doc 00 §7 walks through for
  the x86-64 paging allocator, just applied to registers instead of page
  tables.
- `fence(Ordering::...)` calls around ring-buffer updates
  (`virtio_input.rs`, `use core::sync::atomic::{fence, Ordering}`) are a
  memory-barrier — telling the compiler/CPU "don't reorder memory
  operations across this point" — because the *device* reads the same
  ring buffer concurrently with the CPU; without a fence, the CPU could
  reorder "write the descriptor" after "notify the device the descriptor
  is ready," and the device would read garbage.

---

## How to write a new driver: PC Speaker

The PC speaker is controlled by two things:
- **PIT channel 2** (port 0x42/0x43) — generates a square wave at a given frequency
- **Port 0x61** (system control port B) — bit 0 enables PIT channel 2 output,
  bit 1 enables speaker gate

To play a 440Hz tone (musical 'A'):

```
PIT input clock = 1,193,182 Hz
divisor = 1,193,182 / 440 = 2712

1. Write 0xB6 to port 0x43 (set channel 2, square wave, binary mode)
2. Write low byte of divisor to port 0x42
3. Write high byte of divisor to port 0x42
4. Read port 0x61, set bits 0 and 1, write back
```

To stop:
```
Read port 0x61, clear bits 0 and 1, write back
```

**Your exercise:** Implement this as `kernel/src/kernel/drivers/speaker.rs` with two functions:
- `pub unsafe fn beep(frequency_hz: u32)` — start a tone
- `pub unsafe fn stop()` — silence the speaker

Then add a `beep` command to the terminal: `beep 440` plays a tone.

This forces you to:
- Read a hardware specification and translate it to port I/O
- Deal with the "frequency to divisor" calculation in integer arithmetic
- Handle the "no floating point in kernel" constraint (use integer division)

---

## Reading a hardware datasheet

When you write real driver code, you need a spec. For x86 built-in devices,
the relevant documents are:

| Device | Document to find |
|--------|-----------------|
| 8253/8254 PIT | "Intel 8254 Programmable Interval Timer Datasheet" |
| 8259A PIC | "Intel 8259A Programmable Interrupt Controller" |
| PS/2 keyboard | OSDev wiki: "PS/2 Keyboard" |
| ATA/IDE | "ATA-4 specification" or OSDev "ATA PIO Mode" |
| RTL8139 | "RTL8139 Programming Guide" (Realtek, available online) |
| virtio (any virtio-mmio device) | [VirtIO 1.1 spec](https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.html), §4.2 for the MMIO transport specifically |

OSDev wiki (wiki.osdev.org) is the best starting point for x86 hardware;
for ARM, the QEMU `virt` machine's own docs (`qemu-system-aarch64 -M
virt,help`) plus the VirtIO spec cover everything this port currently uses.

---

## Rust patterns you'll see (Models A–C, x86)

(Full primer: `00_rust_for_os_readers.md`. Model D's patterns are covered
in its own section above.)

- **`asm!` wrapped in tiny, named functions** — `rtc.rs`'s port
  read/write and `pci.rs`'s `pci_read_u32` are one or two lines of
  `unsafe { asm!(...) }` each, never inlined ad-hoc at every call site.
  This is the same "small unsafe core" shape as Model D's `mmio_read32` —
  x86 port I/O and ARM MMIO are different mechanisms wrapped in the
  identical Rust idiom.
- **Bit-flag structs via plain `const` + `|`** — driver init sequences
  (PIC's ICW1–ICW4, the PC-speaker exercise's port-0x61 bits) build
  control-register values by OR-ing named `const` bytes together rather
  than magic numbers, so `0xB6` reads as "channel 2, square wave, binary
  mode" instead of an opaque literal — the same readability goal
  `PageTableFlags` serves in the memory-management code (doc 04).
- **Function pointers as the callback boundary** — `KEY_CALLBACK: Option<
  unsafe fn(u8)>` in `keyboard.rs` is how a low-level driver hands events
  upward without knowing who's listening; doc 03's Rust-patterns section
  covers exactly this value.

---

## Questions

1. Why must an ISR not block (wait for a lock, sleep, or allocate)? What would happen?
2. What is the difference between polling and interrupt-driven I/O? When would you
   choose each?
3. Why does the PIT require an integer divisor? How do you compute frequency from it?
4. What is a "BAR" in PCI? What does it tell you about a device?
5. If a driver needs to transfer a large buffer (e.g., a network packet), and
   the device uses MMIO, why must the writes be `volatile`?

---

## Exercise: PC Speaker driver

Implement `kernel/src/kernel/drivers/speaker.rs` with `beep(freq)` and `stop()`.

Don't look up solutions — read the description in this doc and try. If you get
stuck, use `ata.rs` or `rtc.rs` as models for how port I/O is done in this codebase.

After implementing, add these two terminal commands:
- `beep <hz>` — plays a tone at the given frequency
- `beepstop` — silences it

---

## Your notes
<!-- Add your own notes here as you study -->
