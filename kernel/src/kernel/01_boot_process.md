# 1. The Boot Process

The journey of an operating system begins long before the first line of kernel code is executed. This document details the boot process of OxideOS, from the moment the computer is powered on to the instant the kernel's `kmain` function is called.

---

## Firmware: The First Step

When you turn on a PC, the first software to run is the firmware—either the legacy **BIOS** (Basic Input/Output System) or the modern **UEFI** (Unified Extensible Firmware Interface). The firmware's primary job is to perform a Power-On Self-Test (POST), initialize critical hardware, and then find and transfer control to a **bootloader**.

## The Bootloader: Limine

OxideOS does not run directly on the hardware from the start. It relies on a bootloader to set up a proper execution environment. We use the **Limine Bootloader**.

Limine is a modern, versatile bootloader that simplifies the initial setup for the kernel. Its responsibilities are crucial:

1.  **Loading the Kernel**: Limine locates the kernel's ELF (Executable and Linkable Format) file on the boot disk and loads it into memory.

2.  **Entering 64-bit Long Mode**: It transitions the CPU from its initial 16-bit or 32-bit state into 64-bit "long mode," which is required for a modern operating system.

3.  **Creating Initial Page Tables**: It sets up a basic virtual memory layout. A key part of this is mapping the kernel's code and data into the "higher half" of the virtual address space (e.g., starting at `0xFFFF800000000000`). This separates the kernel's address space from the user-level address space (which will occupy the lower half), preventing collisions.

4.  **Gathering System Information**: Limine probes the hardware and collects vital information that the kernel will need. It passes this information to the kernel using a standardized request/response protocol.

### Limine Requests

The kernel declares its information needs by embedding static "requests" in the kernel executable. Limine finds these requests and provides the corresponding "responses". In `kernel/src/main.rs`, you can see these requests:

```rust
#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();
```

*   `FramebufferRequest`: Asks for a pointer to a linear framebuffer, which is a memory region the kernel can write to for displaying graphics.
*   `MemoryMapRequest`: Asks for a map of the physical memory, detailing which address ranges are usable, reserved by hardware, or unusable. This is essential for the kernel's memory manager.

## Kernel Entry: `kmain()`

Once Limine has completed its setup, it transfers control to the kernel's designated entry point. For OxideOS, this is the `kmain` function in `kernel/src/main.rs`.

The very first tasks in `kmain` are critical for debugging and stability:

1.  **Initialize Serial Port**: `SERIAL_PORT.init()` sets up communication over the serial port. This is often the only way to get debug messages from the kernel in its earliest stages, before the screen is functional.
2.  **Check Limine Revision**: `assert!(BASE_REVISION.is_supported())` ensures that the version of the Limine protocol used by the bootloader is compatible with the version the kernel was compiled against. This prevents subtle bugs from an ABI mismatch.

With these initial steps complete, the kernel is now running and can proceed to initialize its own subsystems, starting with the CPU and interrupt system.