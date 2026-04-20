# Chapter 1: The Boot Process - From Power-On to Kernel Control

The journey of an operating system begins long before the first line of kernel code is executed. This document details the boot process of OxideOS, from the moment the computer is powered on to the instant the kernel's `kmain` function is called.

---

## Firmware: The First Step

When you turn on a PC, the first software to run is the firmware—either the legacy **BIOS** (Basic Input/Output System) or the modern **UEFI** (Unified Extensible Firmware Interface). The firmware's primary job is to perform a Power-On Self-Test (POST), initialize critical hardware, and then find and transfer control to a **bootloader**.

## The Bootloader: Limine

OxideOS does not run directly on the hardware from the start. It relies on a bootloader to set up a proper execution environment. We use the **Limine Bootloader**.

Limine is a modern, versatile bootloader that simplifies the initial setup for the kernel. Its responsibilities are crucial:

1.  **Loading the Kernel**: Limine acts like a delivery service for our kernel. It finds the kernel's executable file (which is in a format called **ELF - Executable and Linkable Format**) on the boot disk (like a CD-ROM or hard drive) and loads its contents into the computer's RAM.

2.  **Entering 64-bit Long Mode**: When a computer first turns on, its CPU starts in a very basic 16-bit mode. Modern operating systems, especially 64-bit ones like OxideOS, need the CPU to operate in a much more powerful 64-bit mode, often called "long mode." Limine handles this complex transition, setting up the CPU so our kernel can use its full capabilities.

3.  **Creating Initial Page Tables**: This is where memory management begins. Limine sets up a basic **virtual memory** system. Think of virtual memory as a map that translates addresses used by programs into actual physical locations in RAM. Limine creates initial "page tables" that map the kernel's code and data into a special region of the virtual address space called the "**higher half**" (e.g., starting at `0xFFFF800000000000`). This is a crucial security and organization step: it ensures the kernel's memory is separate from where user programs will eventually run (the "lower half"), preventing them from accidentally or maliciously interfering with each other.

4.  **Gathering System Information**: Before handing over control, Limine acts as a scout, probing the hardware and collecting vital information that the kernel will need. This includes things like how much RAM is available, where the screen's memory is located, and other hardware details. It then passes this information to the kernel using a standardized **request/response protocol**.

### Limine Requests

Our kernel tells Limine what information it needs by embedding special "requests" directly into its executable code. Limine reads these requests and provides the corresponding "responses." In `kernel/src/main.rs`, you can see how OxideOS declares these needs:

```rust
#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

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