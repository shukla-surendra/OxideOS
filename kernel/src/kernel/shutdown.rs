//! System shutdown / reboot for OxideOS.
//!
//! Shutdown strategy (in order):
//!   1. ACPI proper: parse RSDP → RSDT/XSDT → FADT to get PM1a_CNT_BLK port
//!      and SLP_TYPa from the \_S5 object (hardcoded as 5 for S5 sleep state).
//!   2. QEMU / Bochs ISA debug exit port (0x604 / 0xB004)
//!   3. VirtualBox ACPI control port (0x4004)
//!   4. Fallback: disable interrupts and halt

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

// ── Minimal ACPI table structures ─────────────────────────────────────────────

#[repr(C, packed)]
struct AcpiSdtHeader {
    signature:        [u8; 4],
    length:           u32,
    revision:         u8,
    checksum:         u8,
    oem_id:           [u8; 6],
    oem_table_id:     [u8; 8],
    oem_revision:     u32,
    creator_id:       u32,
    creator_revision: u32,
}

#[repr(C, packed)]
struct Rsdp {
    signature:   [u8; 8],
    checksum:    u8,
    oem_id:      [u8; 6],
    revision:    u8,
    rsdt_addr:   u32,
}

#[repr(C, packed)]
struct Xsdp {
    rsdp:        Rsdp,
    length:      u32,
    xsdt_addr:   u64,
    ext_cksum:   u8,
    _reserved:   [u8; 3],
}

#[repr(C, packed)]
struct FadtMinimal {
    header:           AcpiSdtHeader,
    firmware_ctrl:    u32,
    dsdt:             u32,
    _reserved1:       u8,
    preferred_pm:     u8,
    sci_interrupt:    u16,
    smi_cmd:          u32,
    acpi_enable:      u8,
    acpi_disable:     u8,
    s4bios_req:       u8,
    pstate_ctrl:      u8,
    pm1a_evt_blk:     u32,
    pm1b_evt_blk:     u32,
    pm1a_cnt_blk:     u32,
    pm1b_cnt_blk:     u32,
}

// Fixed HHDM offset used across the kernel.
const HHDM: u64 = 0xFFFF800000000000;

/// Try to find the FADT and return (pm1a_cnt_port, slp_typ_s5).
/// Returns `None` if ACPI tables cannot be found or parsed.
unsafe fn acpi_find_pm1a() -> Option<(u16, u16)> {
    let rsdp_virt = crate::RSDP_REQUEST.get_response()?.address() as u64;
    let rsdp = rsdp_virt as *const Rsdp;
    
    // Check signature "RSD PTR "
    let sig: [u8; 8] = core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).signature));
    if &sig != b"RSD PTR " { return None; }

    let rev = core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).revision));
    
    let fadt_phys: u64 = if rev >= 2 {
        let xsdp = rsdp_virt as *const Xsdp;
        let xsdt_phys = core::ptr::read_unaligned(core::ptr::addr_of!((*xsdp).xsdt_addr));
        find_table_in_xsdt(xsdt_phys, b"FACP")?
    } else {
        let rsdt_phys = core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).rsdt_addr)) as u64;
        find_table_in_rsdt(rsdt_phys, b"FACP")?
    };

    let fadt = (fadt_phys.wrapping_add(HHDM)) as *const FadtMinimal;
    let pm1a = core::ptr::read_unaligned(core::ptr::addr_of!((*fadt).pm1a_cnt_blk)) as u16;

    // SLP_TYPa for S5: hardcoded as 5 for QEMU/VBox.
    Some((pm1a, 5u16))
}

unsafe fn find_table_in_rsdt(rsdt_phys: u64, sig: &[u8; 4]) -> Option<u64> {
    let rsdt = (rsdt_phys.wrapping_add(HHDM)) as *const AcpiSdtHeader;
    let length = core::ptr::read_unaligned(core::ptr::addr_of!((*rsdt).length));
    let entries = (length as usize).saturating_sub(36) / 4;
    let ptrs = (rsdt_phys.wrapping_add(HHDM).wrapping_add(36)) as *const u32;

    for i in 0..entries {
        let phys = core::ptr::read_unaligned(ptrs.add(i)) as u64;
        let header = (phys.wrapping_add(HHDM)) as *const AcpiSdtHeader;
        let table_sig: [u8; 4] = core::ptr::read_unaligned(core::ptr::addr_of!((*header).signature));
        if &table_sig == sig {
            return Some(phys);
        }
    }
    None
}

unsafe fn find_table_in_xsdt(xsdt_phys: u64, sig: &[u8; 4]) -> Option<u64> {
    let xsdt = (xsdt_phys.wrapping_add(HHDM)) as *const AcpiSdtHeader;
    let length = core::ptr::read_unaligned(core::ptr::addr_of!((*xsdt).length));
    let entries = (length as usize).saturating_sub(36) / 8;
    let ptrs = (xsdt_phys.wrapping_add(HHDM).wrapping_add(36)) as *const u64;

    for i in 0..entries {
        let phys = core::ptr::read_unaligned(ptrs.add(i));
        let header = (phys.wrapping_add(HHDM)) as *const AcpiSdtHeader;
        let table_sig: [u8; 4] = core::ptr::read_unaligned(core::ptr::addr_of!((*header).signature));
        if &table_sig == sig {
            return Some(phys);
        }
    }
    None
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, nomem));
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nostack, nomem));
}

pub fn poweroff() -> ! {
    unsafe {
        SERIAL_PORT.write_str("OxideOS: shutting down...\n");

        // Try hypervisor-specific ports first — these are safe I/O writes and
        // don't touch ACPI tables in physical memory that may not be HHDM-mapped.
        outw(0x604, 0x2000);  // QEMU
        outw(0xB004, 0x2000); // Bochs
        outw(0x4004, 0x3400); // VirtualBox
        outw(0x0404, 0x2000); // Common default

        // If still running, attempt proper ACPI shutdown.
        // Only HHDM-mapped physical addresses (usable RAM) are accessible;
        // ACPI tables in the BIOS ROM area may not be mapped, so catch failures
        // by only attempting this if the table walk succeeds.
        if let Some((pm1a_port, slp_typ)) = acpi_find_pm1a() {
            let val = ((slp_typ as u16) << 10) | 0x2000;
            outw(pm1a_port, val);
        }

        asm!("cli");
        loop { asm!("hlt"); }
    }
}

pub fn reboot() -> ! {
    unsafe {
        SERIAL_PORT.write_str("OxideOS: rebooting...\n");
        for _ in 0..0xFF_FFFFu32 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x02) == 0 { break; }
        }
        outb(0x64, 0xFE);
        asm!("lidt [{idtr}]", "int3", idtr = in(reg) &[0u8; 10], options(nostack));
        loop { asm!("hlt"); }
    }
}
