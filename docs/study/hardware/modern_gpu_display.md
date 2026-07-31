# GPU & Display — Raw Framebuffer vs. Real Graphics Hardware

**Source:** `kernel/src/gui/graphics.rs`, `kernel/src/main.rs` (the
`FramebufferRequest` handling in `kmain()`). OxideOS has **no GPU
driver at all** — this doc explains what that means it's *not* doing,
compared to a real display stack.

---

## What OxideOS actually has

At boot, Limine hands `kmain()` a `Framebuffer` — a description of a
linear block of memory the display hardware is already scanning out to
the screen (base address, width, height, pixels-per-scanline, bit depth).
`Graphics::new(framebuffer)` wraps that pointer; every drawing primitive
in this codebase (`fill_rect`, `draw_char`, the window manager's blitting)
is, at the bottom, a direct write into that memory. There is no GPU
driver, no command submission, no shader, no acceleration of any kind —
every pixel on screen was placed there by the CPU writing to RAM that
happens to be what the display is currently reading from.

This is possible because Limine (via **UEFI GOP**, the Graphics Output
Protocol, on x86; an equivalent mechanism on ARM) already asked the
firmware to set up a video mode *before* the kernel ever runs — OxideOS
inherits an already-working display and never has to negotiate a
resolution, timing, or pixel format itself.

---

## What a real GPU stack adds, layer by layer

| Layer | Role | Does OxideOS have this? |
|---|---|---|
| **Scanout / display controller** | Reads the active framebuffer(s) from VRAM, drives display timing | Inherited from firmware, never touched again |
| **Display connectors** (eDP, DisplayPort, HDMI) | Physical link to the panel/monitor — link training, EDID negotiation to discover supported resolutions/refresh rates | Done once by firmware; OxideOS never re-negotiates or even knows this happened |
| **3D/compute engine** | The actual "GPU" — a massively parallel programmable processor (shader cores) executing compiled programs | **Absent entirely.** OxideOS never uses this even on hardware that has one |
| **Command submission** | Userspace (via Metal/Vulkan/DirectX) builds command buffers describing draw calls; submitted to a kernel-mode scheduler that programs GPU-specific ring buffers | Absent — this is the same doorbell-plus-ring-buffer pattern seen in NVMe (`nvme_storage.md`) and xHCI (`usb_xhci.md`), just applied to graphics work instead of storage/USB transfers |
| **Compositor** | Combines multiple app surfaces via the GPU — each window is a GPU texture, composited by shader passes | OxideOS's `window_manager.rs`/`compositor.rs` do a *CPU-side* version of this idea: apps draw into shared buffers via IPC, the kernel composites via `memcpy`-style blitting — the right architecture, running on the one processor that's actually driving pixels here |

---

## Why "GPU" now means something OxideOS never touches

A modern GPU (Apple's own GPU cores in the M-series, Intel Arc/Xe,
AMD RDNA, NVIDIA's architectures) is a second, independent processor with
its own instruction set, its own scheduler, and its own firmware —
graphics APIs compile shader programs down to that GPU's native ISA and
submit them as work packets, conceptually closer to how a NIC's firmware
runs its own protocol stack (`modern_wifi_nics.md`) than to anything a
framebuffer write resembles. None of that ISA or command-stream format is
publicly documented by most vendors; real GPU drivers (Linux's `amdgpu`,
`i915`, Nouveau for NVIDIA, or the Asahi Linux project's driver for Apple
Silicon GPUs) exist because of years of vendor cooperation or reverse
engineering — there is no equivalent of "read the datasheet, write ~600
lines" the way this folder's `rtl8139.md` describes for a NIC.

A **raw scanout framebuffer**, by contrast, is a small, genuinely stable
interface the bootloader/firmware negotiates once — which is exactly why
it's the right scope for a hobby OS, and precisely analogous to why
`docs/study/07_drivers.md`'s progression of drivers stops at things you
can fully understand from a public spec.

---

## Self-check questions

1. What has to happen *before* `kmain()` even runs for `FRAMEBUFFER_REQUEST.get_response()` to succeed? What negotiated the resolution?
2. Why can't OxideOS currently draw anything using hardware acceleration,
   even on a machine with a powerful GPU sitting idle?
3. The compositor pattern (each window as a separate surface, composited
   together) exists in both `window_manager.rs` and every modern desktop
   compositor (macOS WindowServer, Wayland). What's the one thing a real
   GPU compositor does that OxideOS's CPU-side version structurally can't?
4. Why is "write a framebuffer driver" a fundamentally different
   difficulty tier than "write a GPU driver," even though both involve
   putting pixels on a screen?
5. If OxideOS wanted 3D acceleration someday, what would have to exist
   first that has nothing to do with graphics at all? (Hint: think about
   what a GPU driver needs from the memory-management and interrupt
   subsystems this doc series has already covered.)

---

## Sources

- `kernel/src/gui/graphics.rs`, `kernel/src/main.rs`, `docs/study/hardware/modern_wifi_nics.md`, `docs/study/07_drivers.md` (this codebase)
- General GPU architecture concepts (vendor-agnostic; Asahi Linux's public GPU reverse-engineering writeups are a good concrete reference for what "no public docs" driver development looks like)
