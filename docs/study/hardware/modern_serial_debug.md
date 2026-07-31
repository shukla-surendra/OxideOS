# Serial Debug Consoles — 16550 UART vs. What Replaced It

**Companion to:** [16550_uart.md](16550_uart.md) (`kernel/src/kernel/drivers/serial.rs`).
Reference-only.

---

## What it is

COM1's job in OxideOS is entirely about *debugging*, not communication
with a peripheral: every kernel log line goes out over the 16550 so
QEMU/VirtualBox can capture it to a terminal or file, independent of
whether the GUI/framebuffer is even working yet. That's exactly why it's
initialized before the memory allocator (`16550_uart.md`'s own notes) —
it's the kernel's earliest, most dependency-free way to talk to the
outside world. Modern hardware has almost entirely removed the *physical*
16550 while, in one important sense, keeping the *protocol* alive
indefinitely.

---

## What actually happened to the UART

**On consumer laptops, including this MacBook:** nothing replaced it —
it's just gone. No DB9 port, usually no internal header at all. When
something needs low-level debug access to hardware that early, it's a
proprietary mechanism instead: JTAG/SWD debug probes on internal test
points, or (on Apple Silicon specifically) DFU-mode USB and Apple's own
internal diagnostic tooling — none of it standardized, none of it visible
to a third party the way a UART register interface is.

**On servers and workstations:** the UART didn't disappear, it moved
*inside the BMC* (Baseboard Management Controller) — the small,
always-on service processor every server motherboard carries (HPE calls
theirs iLO, Dell calls theirs iDRAC). The BMC exposes **Serial-over-LAN
(SOL)**: it terminates a real (or firmware-emulated) UART locally, then
tunnels that byte stream over the network. From the *server OS's* point
of view, nothing changed — it's still writing to a 16550-compatible
register interface exactly like `serial.rs` does; the BMC is the thing
that decided to forward those bytes over Ethernet instead of a physical
wire.

**On embedded boards and most ARM SoCs:** real UARTs are still common and
still the primary early-boot debug channel — just usually a different
chip design than the 16550. This codebase's own aarch64 port is a direct
example: `arch/aarch64/serial.rs` talks to a **PL011** UART (ARM's own
design, MMIO instead of port I/O, different register layout) rather than
a 16550 — same *role* in the boot sequence (first thing initialized,
serial-only, no allocator dependency), completely different chip.

**USB-to-serial adapters** (FTDI FT232, Silicon Labs CP210x, WCH CH340)
are, in practice, the most common way anyone still talks raw UART to
anything in 2026 — and the trick worth knowing is that these chips
present themselves to the OS as a **virtual 16550-compatible interface**.
A Linux `/dev/ttyUSB0` or macOS `/dev/tty.usbserial-*` device is backed by
a USB driver underneath, but the byte-level protocol and control-line
semantics (RTS/DTR, baud rate, 8N1 framing) it exposes to userspace are
still modeled directly on the 16550's register semantics `serial.rs`
implements — the chip changed, the *interface contract* didn't.

---

## The pattern across all three

| Where it went | What's the same | What changed |
|---|---|---|
| BMC / Serial-over-LAN | Register-level UART protocol, byte-for-byte | Physically tunneled over Ethernet instead of a wire |
| Embedded/ARM (PL011, etc.) | Same *role* in boot (first, dependency-free debug channel) | Different chip, MMIO instead of port I/O, different register layout |
| USB-serial adapters | 16550-compatible logical interface exposed to software | Physical transport is USB; a chip on the dongle does the UART emulation |
| Apple Silicon laptops | — | Genuinely absent; replaced by proprietary, non-standard mechanisms |

The throughline: **the 16550's register-level protocol turned out to be
"good enough, forever," even as the electrical reality under it kept
changing.** That's a different story from every other doc in this folder
— PS/2, ATA, and the 8259A were replaced because their *protocols*
couldn't scale; UART's protocol never needed replacing, only its
transport did (and sometimes not even that).

---

## Why OxideOS uses the 16550 directly

It's the simplest possible early-boot debug channel: no allocator
dependency, no discovery step (COM1 is always at `0x3F8` on any x86 PC),
works identically under QEMU (`-serial stdio`/`-serial file:...`) and
VirtualBox. Given the pattern above, this is arguably the *least* dated
choice of any legacy chip in this comparison series — a real BMC's
Serial-over-LAN implementation is, at the register level, doing
essentially the same thing `serial.rs` does today.

---

## Self-check questions

1. Why would a server vendor choose to keep the 16550's *register
   protocol* in their BMC instead of inventing a cleaner debug interface
   from scratch?
2. `serial.rs`'s init sequence includes a loopback self-test (write
   `0xAE`, read it back). Why does that specific test make less sense for
   a USB-serial adapter than for a real onboard UART?
3. PL011 (ARM) and the 16550 (x86) serve the same *role* in their
   respective boot sequences. Name one concrete implementation difference
   between them beyond "MMIO vs. port I/O."
4. Why is COM1's fixed address (`0x3F8`) something OxideOS can just assume,
   the same way `pci.rs` can't assume a fixed address for a PCI device?
5. If Apple Silicon Macs have no UART-based debug console at all, what
   does that suggest about how Apple's own kernel engineers debug
   early-boot code on real hardware, versus how OxideOS's developer
   debugs it in QEMU?

---

## Sources

- `docs/study/hardware/16550_uart.md`, `kernel/src/kernel/arch/aarch64/serial.rs` (this codebase)
- General BMC/IPMI Serial-over-LAN documentation (industry-standard concept, vendor-specific implementations from HPE/Dell/Lenovo)
