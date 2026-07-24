# Feature 3 (input): virtio-input mouse + keyboard

Status: **done** — mouse and keyboard work in the aarch64 GUI desktop
(verified in QEMU `virt`: cursor motion, window-focus clicks, and typing
commands into the terminal).

## Why virtio-input, and why polled

The x86 build reads input from the 8042 PS/2 controller via port I/O, which
does not exist on ARM. The natural aarch64 sources are USB HID (through
xHCI — a large driver stack) or **virtio-input**, which is a single split
virtqueue delivering 8-byte evdev events. QEMU's `virt` machine provides
virtio-mmio transports out of the box, so virtio-input needs no PCI, no USB
and — crucially at this stage of the port — **no GIC**: like the timer, the
driver is *polled*. The GUI loop already calls `keyboard::poll()` /
`interrupts::poll_mouse_data()` every frame; on aarch64 those drain the
virtio used ring instead of the 8042 output buffer.

The QEMU run targets attach the devices (`Makefile`):

```
-device virtio-keyboard-device -device virtio-mouse-device
```

(The previous `qemu-xhci` + `usb-kbd`/`usb-mouse` devices were removed — the
kernel has no USB stack, so they could never deliver input.)

## Driver layout

`kernel/src/kernel/arch/aarch64/virtio_input.rs`:

1. **Probe** — the `virt` board has 32 virtio-mmio slots at `0x0a00_0000`
   (stride `0x200`), reached through the Limine HHDM (base revision 2 maps
   MMIO; see [02-boot-and-exceptions.md](02-boot-and-exceptions.md)). A slot
   is ours if magic = `"virt"` and device ID = 18 (input). Both legacy
   (version 1 — what QEMU exposes by default) and modern (version 2)
   transports are handled.
2. **Event queue** — queue 0 gets up to 64 descriptors, each pointing at an
   8-byte device-writable `virtio_input_event { type, code, value }` buffer.
   Ring memory comes from the kernel heap, which lives in the HHDM, so
   `phys = virt - hhdm_offset` — no frame allocator needed. The statusq
   (queue 1, keyboard LEDs) is left unused.
3. **Poll** — each GUI frame drains the used ring, dispatches every event,
   and hands the buffer straight back via the avail ring (descriptors are
   never rewritten; descriptor *i* permanently maps buffer *i*).

## Event dispatch — reusing the x86 pipeline

virtio-input events use evdev semantics, and dispatch deliberately funnels
into the code the x86 build already uses:

| evdev event | handling |
|---|---|
| `EV_REL` `REL_X`/`REL_Y` | `MouseCursor::update()` (shared with x86; evdev Y grows downward, PS/2 Y upward, so the sign is flipped) |
| `EV_KEY` `BTN_LEFT/RIGHT/MIDDLE` | button booleans on the shared `PS2Mouse` state the GUI reads |
| `EV_KEY` keyboard keycodes | translated to **scancode-set-1** bytes and fed to `drivers/keyboard::process_scancode` |

The keyboard trick is what keeps the diff small: evdev keycodes 1–88 (the
original XT block) are numerically identical to set-1 make codes, and the
navigation/right-modifier cluster maps to `E0`-prefixed codes. So the whole
`pc-keyboard` decode pipeline — layouts, modifiers, Ctrl-letters, VT100
arrow sequences into stdin, terminal/GUI callbacks — runs unchanged on
aarch64. `drivers/keyboard.rs` is now cross-arch: the decoder and dispatch
compile everywhere; only the 8042 port I/O is `#[cfg(x86_64)]`. The keyboard
stub in `stubs.rs` is gone.

## Gotchas / notes

- **QEMU's virtio-mmio is legacy by default** (`force-legacy=true`): version
  reads 1, the queue is configured via `GuestPageSize`/`QueueAlign`/
  `QueuePFN` with the legacy contiguous ring layout (used ring page-aligned
  after desc+avail). The modern register set is also implemented for other
  hypervisors.
- An idle desktop repaints almost nothing (the taskbar clock has no seconds
  field), so "screen not changing" does **not** mean the kernel hung —
  compare frames across a minute boundary, or move the mouse.
- When the GIC lands, `INT_STATUS`/`INT_ACK` are already handled; switching
  from polling to IRQ is wiring, not a rewrite.
- VMware Fusion is *not* covered yet: it does not place virtio-mmio slots at
  QEMU's addresses. Fusion needs device discovery (device tree / ACPI) and
  most likely a USB HID stack — a later port step.
