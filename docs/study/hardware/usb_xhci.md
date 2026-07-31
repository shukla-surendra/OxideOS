# USB & xHCI — The Bus OxideOS's QEMU Flags Attach but Never Drive

**Source:** none. OxideOS has **zero USB code**. This doc exists because
the project's own `Makefile` already attaches USB hardware to every
x86-64 QEMU boot:

```makefile
$(call USER_VARIABLE,QEMUFLAGS,-m 2G -cpu max -device qemu-xhci,id=xhci -device usb-tablet)
```

Every `make run` on x86-64 boots with a real (emulated) xHCI host
controller and a USB tablet (absolute-positioning pointer) device present
— and nothing in the kernel ever talks to either. The mouse driver
(`kernel/src/kernel/arch/interrupts.rs` + `gui/mouse.rs`) uses the PS/2
mouse instead. This doc explains what's sitting there idle.

---

## What it is

USB replaced a whole family of purpose-built buses — PS/2, serial
(RS-232), parallel (LPT), and dedicated MIDI/joystick ports among them —
with one universal, hot-pluggable, self-describing bus. The trade-off for
that universality is a genuinely large amount of infrastructure before a
single byte reaches an application: a USB keyboard requires far more
machinery to become usable than a PS/2 keyboard ever did, in exchange for
supporting an open-ended variety of device types with the same driver
stack.

---

## The layers

| Layer | Role |
|---|---|
| **Physical bus** | Differential-pair serial signaling; USB 3.2/USB4 push into multi-Gbps territory (see `modern_wifi_nics.md`'s sibling comparisons on Wi-Fi/PCIe for the equivalent jump in those buses) |
| **Host controller** | The chip that actually schedules bus transactions. **xHCI** (eXtensible Host Controller Interface) is the modern universal one — it replaced the old split of UHCI/OHCI (USB 1.1/2.0) and EHCI (2.0 high-speed) with a single controller model that handles USB 1.x/2.0/3.x uniformly |
| **Enumeration** | When a device is plugged in, the host reads its **descriptors** — device descriptor, configuration descriptor, interface descriptor(s), endpoint descriptor(s) — a self-description of what the device is and what data pipes ("endpoints") it exposes |
| **Class drivers** | Generic drivers written against a standardized *class* rather than a specific product: **HID** (keyboards, mice, gamepads — see `modern_input_hid.md`), **Mass Storage/UAS** (drives), **CDC** (serial/network emulation), **Audio** class, etc. A class driver works with *any* compliant device without vendor-specific code |

---

## xHCI's operating model: another ring-buffer-plus-doorbell design

xHCI is structurally the same shape as NVMe's command interface
(`nvme_storage.md`) and virtio's queues (already implemented in this
codebase's `arch/aarch64/virtio_blk.rs`/`virtio_input.rs`, see
`docs/study/07_drivers.md`'s Model D): the driver builds **Transfer
Request Blocks (TRBs)** describing a transfer, places them in a ring
buffer, and rings a **doorbell register**; the controller processes them
asynchronously and reports completions in a separate **Event Ring**,
typically via MSI-X (`pcie_bus.md`). This ring-buffer-plus-doorbell
pattern is worth recognizing as a recurring modern-hardware idiom, not a
coincidence — it's the general solution any sufficiently fast, DMA-capable
device converges on once "the CPU blocks and pokes registers one at a
time" (the ATA-PIO/PS-2 model) stops being fast enough.

**USB HID's "interrupt" polling**, already covered in
[`modern_input_hid.md`](modern_input_hid.md), is where this ring-buffer
model meets input devices specifically: an "Interrupt IN" endpoint isn't
pushed to by the device the way PS/2's IRQ1 is — it's polled by the host
controller's own internal schedule at a negotiated interval, with the
result surfacing through the same TRB/event-ring machinery as any other
USB transfer.

---

## Why this is real, present, unused hardware in every OxideOS boot

`qemu-xhci` and `usb-tablet` are attached specifically because absolute
mouse positioning (a USB tablet reports exact X/Y coordinates) behaves
better than PS/2's relative-motion packets for a GUI — that's a real,
deliberate choice visible in the `Makefile`'s own comment
(`-device usb-tablet: absolute mouse positioning (better than PS/2
relative movements)`). But no driver in this codebase claims that device;
OxideOS's mouse cursor is driven entirely by the PS/2 path instead, which
means the xHCI controller QEMU boots with is fully initialized by QEMU's
firmware/BIOS but never touched by OxideOS's own code at any point after
boot.

Writing even a minimal xHCI + HID-boot-protocol mouse driver would be a
substantially larger undertaking than any driver in `07_drivers.md`'s
existing lineup: host controller initialization alone involves parsing
capability registers, allocating a Device Context Base Address Array,
setting up command/event rings, and running the enumeration
state machine — all *before* a HID report descriptor is even in play.
That complexity ceiling is exactly why `07_drivers.md` never proposes USB
as a "next driver to try" exercise.

---

## Self-check questions

1. Run `make run` (x86-64) and check the QEMU device list — confirm
   `qemu-xhci` and `usb-tablet` are attached. What in the kernel's boot
   log (or lack thereof) proves nothing claims them?
2. Why does xHCI need a Device Context Base Address Array — what problem
   does it solve that a single fixed set of registers (like the 8042's)
   wouldn't?
3. USB HID's "Interrupt IN" endpoint is host-polled, not device-pushed.
   Trace through what that implies for worst-case input latency compared
   to PS/2's real IRQ1, using the polling-interval numbers from
   `modern_input_hid.md`.
4. Name three other buses/interfaces covered in this doc folder that use
   the same "ring buffer + doorbell + completion ring" pattern as xHCI.
   Why does that pattern keep reappearing independently across unrelated
   hardware designs?
5. If OxideOS wanted USB mouse support without writing a full xHCI driver,
   what's the smallest useful subset of the spec that would need
   implementing, and what would still be missing (e.g., hot-plug,
   hubs, other device classes)?

---

## Sources

- `Makefile` (this codebase, `QEMUFLAGS` for x86-64), `docs/study/hardware/modern_input_hid.md`, `docs/study/hardware/nvme_storage.md`, `docs/study/07_drivers.md`
- xHCI is a published Intel specification ("eXtensible Host Controller Interface for Universal Serial Bus"); USB class specs (HID, Mass Storage) are published by usb.org
