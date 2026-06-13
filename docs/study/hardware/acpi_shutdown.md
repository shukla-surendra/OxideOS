# ACPI — Power Management & Shutdown

**Source:** `kernel/src/kernel/drivers/shutdown.rs`

---

## What it is

**ACPI** (Advanced Configuration and Power Interface) is the standard that lets the
OS control system power — sleep, hibernate, shutdown, reboot. It replaced the older
APM standard in the late 1990s.

ACPI works by providing tables in firmware memory that describe the hardware.
To shut down, the OS:
1. Finds the **RSDP** (Root System Description Pointer) — the entry point
2. Walks to the **RSDT** or **XSDT** table
3. Finds the **FADT** (Fixed ACPI Description Table)
4. Reads `PM1a_CNT_BLK` — the port number for the power management control register
5. Writes a sleep command to that port

---

## Shutdown strategies in OxideOS

`shutdown.rs` tries multiple methods in order, from easiest to hardest:

```rust
pub fn poweroff() -> ! {
    // 1. Hypervisor-specific shortcut ports (no ACPI parsing needed)
    outw(0x604, 0x2000);   // QEMU
    outw(0xB004, 0x2000);  // Bochs
    outw(0x4004, 0x3400);  // VirtualBox
    outw(0x0404, 0x2000);  // Generic fallback

    // 2. Proper ACPI shutdown (if ACPI tables are accessible)
    if let Some((pm1a_port, slp_typ)) = acpi_find_pm1a() {
        let val = ((slp_typ as u16) << 10) | 0x2000;
        outw(pm1a_port, val);
    }

    // 3. Last resort: halt the CPU with interrupts disabled
    asm!("cli");
    loop { asm!("hlt"); }
}
```

---

## Method 1: Hypervisor ports

QEMU, Bochs, and VirtualBox each support a non-standard "magic" I/O port that
immediately powers off the VM when you write a specific value:

| Port | Value | Hypervisor |
|------|-------|------------|
| `0x604` | `0x2000` | QEMU |
| `0xB004` | `0x2000` | Bochs |
| `0x4004` | `0x3400` | VirtualBox |
| `0x0404` | `0x2000` | Generic |

These aren't real ACPI — they're debug features baked into the emulator.
Writing them is safe on real hardware (the writes do nothing on real PCs because
nothing is mapped at those ports).

---

## Method 2: Proper ACPI shutdown

### ACPI table hierarchy

```
RSDP (Root System Description Pointer)
  │ — found via Limine bootloader request
  │ — contains physical address of RSDT or XSDT
  ▼
RSDT (Root System Description Table)  — ACPI 1.0
  or
XSDT (Extended System Description Table)  — ACPI 2.0+
  │ — array of physical addresses of other tables
  ▼
FADT (Fixed ACPI Description Table, signature "FACP")
  │ — contains PM1a_CNT_BLK: the port address for PM1 control
  ▼
PM1a_CNT_BLK port
  — write (SLP_TYPa << 10) | SLP_EN to initiate sleep/shutdown
```

### RSDP → RSDT/XSDT

The RSDP contains:
- `signature: [u8; 8]` — must be `"RSD PTR "` (with trailing space)
- `revision: u8` — 0 = ACPI 1.0 (use RSDT), ≥2 = ACPI 2.0 (use XSDT)
- `rsdt_addr: u32` — physical address of RSDT (ACPI 1.0)
- `xsdt_addr: u64` (in XSDP extension) — physical address of XSDT (ACPI 2.0)

```rust
// shutdown.rs line 78-87
let rev = core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).revision));
let fadt_phys = if rev >= 2 {
    find_table_in_xsdt(xsdt_phys, b"FACP")?   // ACPI 2.0+
} else {
    find_table_in_rsdt(rsdt_phys, b"FACP")?   // ACPI 1.0
};
```

### Finding a table

The RSDT/XSDT is an array of physical pointers to other SDTs. Each SDT starts with
a common header that includes a 4-byte signature. To find the FADT (signature `"FACP"`):

```rust
// shutdown.rs find_table_in_rsdt() — line 96
for i in 0..entries {
    let phys = read_unaligned(ptrs.add(i)) as u64;
    let header = (phys + HHDM) as *const AcpiSdtHeader;
    let table_sig: [u8; 4] = read_unaligned(addr_of!((*header).signature));
    if &table_sig == b"FACP" { return Some(phys); }
}
```

`HHDM = 0xFFFF800000000000` — the higher-half direct map offset. Physical addresses
are not directly accessible in the kernel's virtual address space; adding HHDM converts
a physical address to the kernel's virtual mapping of it.

### The PM1a control register

Once the FADT is found, read `PM1a_CNT_BLK`:
```rust
// shutdown.rs line 90
let pm1a = read_unaligned(addr_of!((*fadt).pm1a_cnt_blk)) as u16;
```

To trigger S5 (soft off / shutdown):
```rust
// SLP_TYPa for S5 = 5 (hardcoded for QEMU/VBox)
// SLP_EN = bit 13 = 0x2000
let val = ((5u16) << 10) | 0x2000;
outw(pm1a_port, val);
```

The hardware interprets this write as "enter sleep state S5" and powers off.

---

## Reboot via 8042

`shutdown.rs reboot()` uses the PS/2 controller to reset the CPU:

```rust
// Wait for 8042 input buffer empty (bit 1 of port 0x64)
loop {
    let status: u8;
    asm!("in al, 0x64", ...);
    if (status & 0x02) == 0 { break; }
}
// Command 0xFE: pulse the CPU RESET line
outb(0x64, 0xFE);
```

The 8042 microcontroller has a dedicated output pin wired to the CPU's RESET line.
Command `0xFE` tells it to assert that pin for ~6 µs — instantly cold-boots the machine.

The `int3` fallback after this handles any CPU that ignores the 8042 reset:
```rust
// Load a null IDT and trigger an exception → triple fault → reboot
asm!("lidt [{idtr}]", "int3", idtr = in(reg) &[0u8; 10], ...);
```

---

## `#[repr(C, packed)]` on ACPI structs

ACPI tables are defined by firmware, not by us. Their fields may not be
naturally aligned. `#[repr(C, packed)]` tells Rust not to add padding bytes:

```rust
#[repr(C, packed)]
struct Rsdp {
    signature: [u8; 8],
    checksum:  u8,
    oem_id:    [u8; 6],
    revision:  u8,
    rsdt_addr: u32,   // might be at an odd offset due to preceding fields
}
```

Reading fields of a packed struct requires `read_unaligned()` — accessing them
directly would cause an alignment fault because the compiler can't guarantee alignment.

---

## Common gotchas

**1. ACPI tables live in physical memory the kernel may not have mapped.**
The HHDM (higher-half direct map) only covers usable RAM. ACPI tables stored in
firmware ROM or ACPI-reserved regions may not be accessible. OxideOS handles this
by trying the hypervisor ports first (which are always safe) and only attempting
ACPI if the table walk succeeds.

**2. `SLP_TYPa` is system-specific.**
The correct value for S5 should be read from the `\_S5` AML object in the DSDT
table. OxideOS hardcodes `5` which works for QEMU and VirtualBox but may not
work on all real hardware.

**3. Writing to unknown ports is safe but does nothing on real hardware.**
The hypervisor-specific ports (`0x604`, `0xB004`, etc.) are invisible on real
machines — they're in the PCI I/O space but nothing is mapped there.

---

## Self-check questions

1. Why does OxideOS try hypervisor ports before proper ACPI? What's the advantage?
2. What is the HHDM offset and why do physical addresses need it added before use?
3. What does `#[repr(C, packed)]` do? Why is `read_unaligned()` needed with it?
4. Why is triple-faulting a valid reboot strategy? What happens when a CPU triple-faults?
5. What would you need to change to support a real PC that uses a non-standard S5 type?
