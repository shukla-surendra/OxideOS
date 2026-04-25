//! Minimal PCI configuration-space access (port I/O method).
//!
//! The PCI host bridge exposes two 32-bit ports:
//!   0xCF8 — CONFIG_ADDRESS (write the target address)
//!   0xCFC — CONFIG_DATA    (read/write the 32-bit register)
//!
//! Address format: [31]=enable, [30:24]=0, [23:16]=bus, [15:11]=device,
//!                 [10:8]=function, [7:2]=register, [1:0]=0

use core::arch::asm;

const PCI_CFG_ADDR: u16 = 0xCF8;
const PCI_CFG_DATA: u16 = 0xCFC;

/// Build the 32-bit PCI CONFIG_ADDRESS value.
fn pci_addr(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    (1u32 << 31)
        | ((bus  as u32) << 16)
        | ((dev  as u32) << 11)
        | ((func as u32) << 8)
        | ((offset & 0xFC) as u32)
}

fn outl(port: u16, val: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack)); }
}
fn inl(port: u16) -> u32 {
    let v: u32;
    unsafe { asm!("in eax, dx", out("eax") v, in("dx") port, options(nomem, nostack)); }
    v
}

/// Read a 32-bit PCI config register.
pub fn read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    outl(PCI_CFG_ADDR, pci_addr(bus, dev, func, offset));
    inl(PCI_CFG_DATA)
}

/// Write a 32-bit PCI config register.
pub fn write32(bus: u8, dev: u8, func: u8, offset: u8, val: u32) {
    outl(PCI_CFG_ADDR, pci_addr(bus, dev, func, offset));
    outl(PCI_CFG_DATA, val);
}

/// Read a 16-bit PCI config register.
pub fn read16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let dword = read32(bus, dev, func, offset & !2);
    if offset & 2 == 0 { dword as u16 } else { (dword >> 16) as u16 }
}

/// Write a 16-bit PCI config register.
pub fn write16(bus: u8, dev: u8, func: u8, offset: u8, val: u16) {
    let aligned = offset & !2;
    let mut dword = read32(bus, dev, func, aligned);
    if offset & 2 == 0 {
        dword = (dword & 0xFFFF0000) | (val as u32);
    } else {
        dword = (dword & 0x0000FFFF) | ((val as u32) << 16);
    }
    write32(bus, dev, func, aligned, dword);
}

/// A located PCI device.
#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    pub bus:    u8,
    pub dev:    u8,
    pub func:   u8,
    pub vendor: u16,
    pub device: u16,
}

impl PciDevice {
    /// Read a BAR and return its I/O base (if it is an I/O BAR).
    pub fn io_bar(&self, bar: u8) -> Option<u16> {
        let reg = 0x10 + bar * 4;
        let val = read32(self.bus, self.dev, self.func, reg);
        if val & 1 != 0 { Some((val & 0xFFFC) as u16) } else { None }
    }

    /// Enable PCI bus mastering (needed for DMA).
    pub fn enable_bus_mastering(&self) {
        let cmd = read16(self.bus, self.dev, self.func, 0x04);
        write16(self.bus, self.dev, self.func, 0x04, cmd | 0x0007);
        // bits: I/O space (0), memory space (1), bus master (2)
    }
}

/// Scan all buses for a device matching `vendor_id:device_id`.
pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            // Check all 8 functions in case it's a multi-function device.
            let hdr = read32(bus, dev, 0, 0x0C);
            let max_func: u8 = if (hdr >> 23) & 1 != 0 { 8 } else { 1 };
            for func in 0..max_func {
                let id = read32(bus, dev, func, 0x00);
                let vendor = id as u16;
                if vendor == 0xFFFF { continue; }
                let device = (id >> 16) as u16;
                if vendor == vendor_id && device == device_id {
                    return Some(PciDevice { bus, dev, func, vendor, device });
                }
            }
        }
    }
    None
}

/// Print every detected PCI device (vendor:device, bus/dev/func) to serial.
/// Call this once at boot to help diagnose missing hardware.
pub fn enumerate_to_serial() {
    unsafe {
        let sp = &crate::kernel::serial::SERIAL_PORT;
        sp.write_str("[pci] scan:\n");
        for bus in 0..=255u8 {
            for dev in 0..32u8 {
                let id0 = read32(bus, dev, 0, 0x00);
                if id0 as u16 == 0xFFFF { continue; }
                let hdr = read32(bus, dev, 0, 0x0C);
                let max_func: u8 = if (hdr >> 23) & 1 != 0 { 8 } else { 1 };
                for func in 0..max_func {
                    let id = read32(bus, dev, func, 0x00);
                    let vendor = id as u16;
                    if vendor == 0xFFFF { continue; }
                    let device = (id >> 16) as u16;
                    let class_dword = read32(bus, dev, func, 0x08);
                    let class = (class_dword >> 24) as u8;
                    let sub   = (class_dword >> 16) as u8;
                    sp.write_str("  [");
                    sp.write_hex(bus as u32); sp.write_str(":");
                    sp.write_hex(dev as u32); sp.write_str(".");
                    sp.write_hex(func as u32);
                    sp.write_str("] ");
                    sp.write_hex(vendor as u32); sp.write_str(":");
                    sp.write_hex(device as u32);
                    sp.write_str(" class=");
                    sp.write_hex(class as u32); sp.write_str(":");
                    sp.write_hex(sub as u32);
                    sp.write_str("\n");
                }
            }
        }
        sp.write_str("[pci] scan done\n");
    }
}
