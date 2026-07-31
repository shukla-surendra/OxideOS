# Hardware Reference — OxideOS

One document per physical chip or bus that OxideOS talks to directly.
Each doc explains what the hardware does, its I/O ports and registers,
its initialization sequence, and exactly how OxideOS uses it — with
line-level references to the source code.

| Chip / Bus | Source File | Doc |
|---|---|---|
| 8259A PIC — Interrupt Controller | `drivers/pic.rs` | [8259A_pic.md](8259A_pic.md) |
| 8253/8254 PIT — Timer | `drivers/timer.rs` | [8253_pit.md](8253_pit.md) |
| Intel 8042 — PS/2 Controller & Keyboard | `drivers/keyboard.rs` | [8042_ps2_keyboard.md](8042_ps2_keyboard.md) |
| NS 16550 UART — Serial Port | `drivers/serial.rs` | [16550_uart.md](16550_uart.md) |
| CMOS / MC146818 — Real-Time Clock | `drivers/rtc.rs` | [cmos_rtc.md](cmos_rtc.md) |
| ATA / IDE — Disk Controller | `drivers/ata.rs` | [ata_ide.md](ata_ide.md) |
| PCI Bus — Device Discovery | `drivers/net/pci.rs` | [pci_bus.md](pci_bus.md) |
| RTL8139 — Ethernet NIC | `drivers/net/rtl8139.rs` | [rtl8139.md](rtl8139.md) |
| ACPI — Power Management / Shutdown | `drivers/shutdown.rs` | [acpi_shutdown.md](acpi_shutdown.md) |

## Modern hardware, compared

*(Reference only — real-world hardware OxideOS doesn't drive, or a modern
successor to a chip above. Each doc explains what changed and why, with
the legacy chip above as the baseline comparison.)*

| Topic | Compares against | Doc |
|---|---|---|
| Wi-Fi NICs (Broadcom, Intel BE200, MediaTek, Qualcomm) | RTL8139 | [modern_wifi_nics.md](modern_wifi_nics.md) |
| Interrupt controllers (APIC/x2APIC, ARM GICv3/v4, Apple AIC) | 8259A PIC | [interrupt_controller_modern.md](interrupt_controller_modern.md) |
| Timers (HPET, LAPIC timer, TSC-deadline, ARM generic timer) | 8253/8254 PIT | [modern_timers.md](modern_timers.md) |
| Input (USB HID, Apple SPI/DockChannel transport) | 8042 PS/2 | [modern_input_hid.md](modern_input_hid.md) |
| Storage (NVMe over PCIe) | ATA/IDE PIO | [nvme_storage.md](nvme_storage.md) |
| Debug consoles (BMC Serial-over-LAN, PL011, USB-serial) | 16550 UART | [modern_serial_debug.md](modern_serial_debug.md) |
| Real-time clocks (PMIC RTC, UEFI time services) | CMOS/MC146818 | [modern_rtc.md](modern_rtc.md) |
| PCIe (ECAM, lanes/generations, MSI-X) | Legacy PCI | [pcie_bus.md](pcie_bus.md) |
| GPU & display architecture | OxideOS's raw framebuffer (no GPU driver) | [modern_gpu_display.md](modern_gpu_display.md) |
| USB & xHCI | No OxideOS driver (QEMU flags attach one, unused) | [usb_xhci.md](usb_xhci.md) |
| Memory (DDR5/LPDDR5X, memory controllers, ECC) | OxideOS's flat physical-map allocator | [modern_memory.md](modern_memory.md) |

## How to read the driver docs (chips OxideOS actually implements)

Each document follows this structure:
1. **What it is** — one-paragraph plain-English description
2. **I/O ports** — every port the driver touches, with hex address and purpose
3. **Key registers** — the important bits/fields inside those ports
4. **Initialization sequence** — what the driver does at boot, step by step
5. **Runtime operation** — how it works during normal OS operation
6. **In OxideOS** — exact functions and line numbers in this codebase
7. **Common gotchas** — things that trip people up

The "modern hardware, compared" docs follow a different shape — a
side-by-side comparison table, "why the shift happened," and "why OxideOS
still uses the legacy version" — since there's no OxideOS driver for most
of them to document line-by-line.

## Port map — all hardware at a glance

```
0x0020 – 0x0021   8259A PIC master (command / data)
0x0040 – 0x0043   8253 PIT channels 0–2 + mode register
0x0060            PS/2 data port (keyboard & mouse data)
0x0061            System control port B (speaker gate, NMI mask)
0x0064            PS/2 status/command port (8042 controller)
0x0070 – 0x0071   CMOS index / data (RTC + CMOS RAM)
0x0080            I/O delay port (POST diagnostic)
0x00A0 – 0x00A1   8259A PIC slave (command / data)
0x01F0 – 0x01F7   ATA primary bus I/O registers
0x03F6            ATA primary bus control register
0x0170 – 0x0177   ATA secondary bus I/O registers
0x0376            ATA secondary bus control register
0x03F8 – 0x03FF   NS16550 UART COM1 (serial port)
0x0404            Power-off port (generic fallback)
0x0604            QEMU power-off port
0x4004            VirtualBox power-off port
0xB004            Bochs power-off port
0x0CF8            PCI CONFIG_ADDRESS
0x0CFC            PCI CONFIG_DATA
0x00A0 – 0x00A1   8259A PIC slave
```
