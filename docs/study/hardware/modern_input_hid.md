# Input Devices — 8042 PS/2 vs. USB HID vs. Apple's SPI Transport

**Companion to:** [8042_ps2_keyboard.md](8042_ps2_keyboard.md)
(`kernel/src/kernel/drivers/keyboard.rs`). Reference-only.

---

## What it is

PS/2 conflates two things that modern designs deliberately keep separate:
the **electrical transport** (how bytes physically move) and the
**protocol** (what the bytes mean). On PS/2, "scancode set 1/2" *is* the
wire format — there's no layer beneath it. Every modern replacement splits
these apart, which is the single most important thing to understand about
why input hardware looks so different now.

---

## USB HID: protocol and transport, cleanly separated

**HID (Human Interface Device)** is a USB *device class* — a
standardized, self-describing protocol any input device can implement,
regardless of what it physically is (keyboard, mouse, gamepad, touch
digitizer). A HID device ships a **report descriptor**: a small
byte-string the device sends once, during enumeration, that tells the host
exactly how to interpret the fixed-size **reports** it'll send afterward
("byte 0 bit 0 = left mouse button," "bytes 1–2 = signed 16-bit X delta,"
etc.) — nothing is hardcoded the way PS/2's scancode tables are.

| | PS/2 | USB HID |
|---|---|---|
| Protocol tied to transport? | Yes — scancodes are the wire format | No — HID reports ride on top of any USB transfer |
| Device self-describes? | No — host must already know the scancode set | Yes — report descriptor sent at enumeration |
| Delivery model | Real hardware IRQ (device asserts IRQ1/IRQ12 the instant it has data) | **Polled by the host controller** on a fixed schedule (an "Interrupt IN" endpoint, despite the name, is the host asking "anything new?" at a negotiated interval — not a real interrupt) |
| Multiplexing | 2 fixed ports (keyboard, mouse) via one 8042 chip | Arbitrarily many devices, all sharing the same protocol layer, distinguished by USB address/endpoint |

That "Interrupt IN" naming is a common point of confusion: in USB, it does
**not** mean the device fires an electrical interrupt the way PS/2's IRQ1
does. It means the host controller (xHCI — see
[`usb_xhci.md`](usb_xhci.md)) polls that endpoint at a fixed interval
(commonly 1–8 ms for a keyboard/mouse) as part of its normal schedule.
Input latency on USB HID is therefore bounded by the polling interval, not
truly interrupt-driven at the hardware level — a real, if usually
imperceptible, architectural trade-off PS/2's genuine push model didn't
have.

---

## The MacBook this doc series was researched on: neither PS/2 nor USB

This is the least widely known part of the comparison, and it's directly
relevant here since the earlier `modern_wifi_nics.md` doc in this folder
was researched on a MacBook Pro M4 Pro. Apple's **internal** keyboard and
trackpad — on every MacBook since ~2015 — do **not** use USB at all. They
sit on **SPI** (Serial Peripheral Interface, a simple synchronous
point-to-point bus, unrelated to PCI despite the acronym collision), which
macOS exposes through a driver literally named
`AppleHIDTransportHIDDevice`. On the latest Apple Silicon models, this has
evolved further: a **DockChannel** transport boots a small embedded
coprocessor (via Apple's RTKit framework) that owns both the keyboard and
trackpad, encapsulating the HID protocol inside a mailbox interface to
that coprocessor rather than exposing SPI registers directly to the main
CPU.

The important takeaway: **HID as a logical protocol is transport-agnostic
by design** — the exact same report-descriptor/report model runs over
USB, over Bluetooth (BLE HID, most wireless keyboards/mice), over I2C
(most Windows laptop touchpads, "I2C-HID"), and over Apple's own SPI/
mailbox transport. PS/2 has no equivalent split — there's no way to run
"PS/2 protocol" over a different bus, because the protocol *is* the bus.

---

## Why OxideOS still uses PS/2

Same reasoning threaded through every doc in this folder: PS/2 needs no
enumeration step (you already know there's a keyboard at port `0x60`,
full stop — see `8042_ps2_keyboard.md`'s init sequence), no descriptor
parsing, no host-controller scheduling model. It also happens to be
exactly what OxideOS's own aarch64 port worked around rather than
replicated: `arch/aarch64/virtio_input.rs` (see
`docs/study/07_drivers.md`'s Model D) deliberately **translates its
virtio-input evdev events back into PS/2 scancode-set-1 bytes** and feeds
them into the *same* `keyboard::process_scancode()` this doc's companion
describes — reusing the whole downstream pipeline (modifier tracking,
callbacks, the terminal's event queue) instead of building a second,
HID-shaped one. That choice is a small-scale echo of the exact same
protocol/transport question this doc is about: OxideOS picked to keep one
protocol (scancode-set-1) and swap only the transport underneath it.

---

## Self-check questions

1. Why does a USB keyboard need to send a report *descriptor* before the
   host can make sense of its reports, when a PS/2 keyboard needs nothing
   equivalent?
2. USB's "Interrupt IN" endpoint type is host-polled, not device-pushed.
   What's one concrete latency consequence of that, compared to PS/2's
   real IRQ1?
3. Apple's internal keyboard uses neither PS/2 nor USB. What does that
   imply for a hypothetical OxideOS port targeting real Apple Silicon
   hardware, versus targeting QEMU (which emulates PC-standard PS/2 or
   USB HID)?
4. `virtio_input.rs` translates evdev events into PS/2 scancode-set-1
   bytes to reuse `keyboard.rs`'s existing pipeline. What's the trade-off
   of that choice versus writing a native HID-report-shaped input path?
5. I2C-HID (common on Windows touchpads) and Apple's SPI transport both
   avoid USB for internal devices. What do I2C and SPI have in common that
   makes them a better fit for a device permanently soldered inside the
   same chassis, versus USB's original design target (external,
   hot-pluggable peripherals)?

---

## Sources

- [SPI trackpads — mac-precision-touchpad #171](https://github.com/imbushuo/mac-precision-touchpad/issues/171)
- [New Linux Driver Posted To Enable Keyboard Support On M3 MacBooks — Phoronix](https://www.phoronix.com/news/Apple-DockChannel-M3-Keyboard)
- [HIDDriverKit — Apple Developer Documentation](https://developer.apple.com/documentation/hiddriverkit)
- `docs/study/hardware/8042_ps2_keyboard.md`, `docs/study/07_drivers.md` (this codebase)
