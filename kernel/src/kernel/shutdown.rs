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

/// Generic ACPI System Descriptor Table (SDT) header (36 bytes).
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

/// RSDP v1 (20 bytes, "RSD PTR " signature).
#[repr(C, packed)]
struct Rsdp {
    signature:   [u8; 8],
    checksum:    u8,
    oem_id:      [u8; 6],
    revision:    u8,
    rsdt_addr:   u32,  // RSDT physical address (always valid for revision < 2)
}

/// RSDP v2 extension (XSDP, adds 64-bit XSDT address).
#[repr(C, packed)]
struct Xsdp {
    rsdp:        Rsdp,
    length:      u32,
    xsdt_addr:   u64,
    ext_cksum:   u8,
    _reserved:   [u8; 3],
}

/// FADT (Fixed ACPI Description Table), minimal fields we need.
/// Full table is larger; we only read what we need.
#[repr(C, packed)]
struct FadtMinimal {
    header:           AcpiSdtHeader,  // 36 bytes
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
    pm1a_evt_blk:     u32,   // offset 56
    pm1b_evt_blk:     u32,   // offset 60
    pm1a_cnt_blk:     u32,   // offset 64  ← PM1a control block I/O port
    pm1b_cnt_blk:     u32,   // offset 68
}

// HHDM offset — Limine maps all physical memory here.
const HHDM: u64 = 0xFFFF_8000_0000_0000;

/// Convert a physical address to a kernel virtual pointer via HHDM.
unsafe fn phys_to_virt<T>(phys: u64) -> *const T {
    (phys + HHDM) as *const T
}

/// Try to find the FADT and return (pm1a_cnt_port, slp_typ_s5).
/// Returns `None` if ACPI tables cannot be found or parsed.
unsafe fn acpi_find_pm1a() -> Option<(u16, u16)> {
    // Limine gives us a virtual (HHDM-mapped) pointer to the RSDP.
    let rsdp_virt = {
        let resp = crate::RSDP_REQUEST.get_response()?;
        resp.address() as u64
    };

    // Convert the Limine virtual address back to a physical address.
    let rsdp_phys = rsdp_virt.wrapping_sub(HHDM);

    let rsdp = unsafe { &*phys_to_virt::<Rsdp>(rsdp_phys) };
    // Validate signature (packed struct — use copy semantics).
    let sig: [u8; 8] = unsafe { core::ptr::read_unaligned(&rsdp.signature) };
    if &sig != b"RSD PTR " { return None; }

    let rev = unsafe { core::ptr::read_unaligned(&rsdp.revision) };
    unsafe { SERIAL_PORT.write_str("ACPI: RSDP found, rev="); }
    unsafe { SERIAL_PORT.write_decimal(rev as u32); }
    unsafe { SERIAL_PORT.write_str("\n"); }

    // Walk either XSDT (v2+) or RSDT (v1) to find FACP (FADT).
    let fadt_phys: u64 = if rev >= 2 {
        let xsdp_ptr = unsafe { phys_to_virt::<Xsdp>(rsdp_phys) };
        let xsdt_phys = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*xsdp_ptr).xsdt_addr)) };
        find_table_in_xsdt(xsdt_phys, b"FACP")?
    } else {
        let rsdp_ptr = unsafe { phys_to_virt::<Rsdp>(rsdp_phys) };
        let rsdt_phys = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp_ptr).rsdt_addr)) } as u64;
        find_table_in_rsdt(rsdt_phys, b"FACP")?
    };

    let fadt_ptr = unsafe { phys_to_virt::<FadtMinimal>(fadt_phys) };
    let pm1a = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*fadt_ptr).pm1a_cnt_blk)) } as u16;

    unsafe { SERIAL_PORT.write_str("ACPI: PM1a_CNT_BLK=0x"); }
    unsafe { SERIAL_PORT.write_hex(pm1a as u32); }
    unsafe { SERIAL_PORT.write_str("\n"); }

    // SLP_TYPa for S5: QEMU/VBox/OVMF all use value 5 (or 0 for some firmwares).
    // A complete implementation would parse the DSDT AML \_S5 package.
    // Hardcoding 5 is correct for QEMU + SeaBIOS and VirtualBox.
    Some((pm1a, 5u16))
}

/// Walk an RSDT (32-bit entry pointers) for a table with the given signature.
unsafe fn find_table_in_rsdt(rsdt_phys: u64, sig: &[u8; 4]) -> Option<u64> {
    let hdr_ptr = unsafe { phys_to_virt::<AcpiSdtHeader>(rsdt_phys) };
    let hdr_sig: [u8; 4] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr_ptr).signature)) };
    if &hdr_sig != b"RSDT" { return None; }
    let len = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr_ptr).length)) } as usize;
    let entries = (len.saturating_sub(36)) / 4;
    let entry_base = (rsdt_phys + HHDM + 36) as *const u32;
    for i in 0..entries {
        let phys = unsafe { core::ptr::read_unaligned(entry_base.add(i)) } as u64;
        let h2 = unsafe { phys_to_virt::<AcpiSdtHeader>(phys) };
        let s: [u8; 4] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*h2).signature)) };
        if &s == sig { return Some(phys); }
    }
    None
}

/// Walk an XSDT (64-bit entry pointers) for a table with the given signature.
unsafe fn find_table_in_xsdt(xsdt_phys: u64, sig: &[u8; 4]) -> Option<u64> {
    let hdr_ptr = unsafe { phys_to_virt::<AcpiSdtHeader>(xsdt_phys) };
    let hdr_sig: [u8; 4] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr_ptr).signature)) };
    if &hdr_sig != b"XSDT" { return None; }
    let len = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr_ptr).length)) } as usize;
    let entries = (len.saturating_sub(36)) / 8;
    let entry_base = (xsdt_phys + HHDM + 36) as *const u64;
    for i in 0..entries {
        let phys = unsafe { core::ptr::read_unaligned(entry_base.add(i)) };
        let h2 = unsafe { phys_to_virt::<AcpiSdtHeader>(phys) };
        let s: [u8; 4] = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*h2).signature)) };
        if &s == sig { return Some(phys); }
    }
    None
}

/// Write a 16-bit value to an I/O port.
#[inline]
unsafe fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, nomem)); }
}

/// Write an 8-bit value to an I/O port.
#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nostack, nomem)); }
}

/// Power off the machine.  Never returns under normal circumstances.
pub fn poweroff() -> ! {
    unsafe {
        SERIAL_PORT.write_str("OxideOS: shutting down...\n");

        // 1. ACPI proper: parse RSDP→FADT to get PM1a_CNT port and SLP_TYPa.
        //    SLP_EN (bit 13) | SLP_TYPa (bits 10–12) = (slp_typ << 10) | 0x2000
        if let Some((pm1a_port, slp_typ)) = acpi_find_pm1a() {
            let val = ((slp_typ as u16) << 10) | 0x2000;
            SERIAL_PORT.write_str("ACPI: writing 0x");
            SERIAL_PORT.write_hex(val as u32);
            SERIAL_PORT.write_str(" to PM1a port 0x");
            SERIAL_PORT.write_hex(pm1a_port as u32);
            SERIAL_PORT.write_str("\n");
            outw(pm1a_port, val);
        }

        // 2. QEMU ISA debug / power-off device (iobase 0x604, value 0x2000)
        outw(0x604, 0x2000);

        // 3. Bochs power-off port
        outw(0xB004, 0x2000);

        // 4. VirtualBox ACPI PM1a control register (port 0x4004)
        outw(0x4004, 0x3400);

        // 5. ACPI PM1a_CNT at 0x404 (common firmware default)
        outw(0x0404, 0x2000);

        // 6. Final fallback: disable interrupts and halt.
        asm!("cli");
        loop { asm!("hlt", options(nostack, nomem)); }
    }
}

/// Reboot the machine via the keyboard controller (classic 8042 reset).
pub fn reboot() -> ! {
    unsafe {
        SERIAL_PORT.write_str("OxideOS: rebooting...\n");

        // Pulse the reset line via 8042 output port.
        // Wait for input buffer empty first.
        for _ in 0..0xFF_FFFFu32 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x02) == 0 { break; }
        }
        outb(0x64, 0xFE); // pulse reset

        // If that didn't work, try triple fault.
        asm!(
            "lidt [{idtr}]",
            "int3",
            idtr = in(reg) &[0u8; 10],
            options(nostack)
        );

        loop { asm!("hlt", options(nostack, nomem)); }
    }
}
