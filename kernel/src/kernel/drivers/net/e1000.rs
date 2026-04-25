//! Intel 82540EM (e1000) Ethernet driver.
//!
//! Supports VirtualBox "Intel PRO/1000 MT Desktop (82540EM)" — the default
//! VirtualBox NAT adapter.  Uses I/O port register access via BAR2
//! (IOADDR at io_base+0, IODATA at io_base+4) — no MMIO/HHDM mapping needed.
//!
//! Also detected: 82545EM (0x100F), 82543GC (0x1004), 82541PI (0x107C).

extern crate alloc;

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};
use core::arch::asm;
use super::pci;

// ── Supported PCI device IDs (all vendor 0x8086) ─────────────────────────────

const VENDOR: u16 = 0x8086;
const DEVICES: &[u16] = &[
    0x100E, // 82540EM  — VirtualBox default
    0x100F, // 82545EM
    0x1004, // 82543GC
    0x107C, // 82541PI
    0x10D3, // 82574L
    0x1539, // I211
];

// ── Register offsets (same for MMIO and IOADDR/IODATA) ───────────────────────

const CTRL:  u32 = 0x0000;
const IMC:   u32 = 0x00D8;
const RCTL:  u32 = 0x0100;
const TCTL:  u32 = 0x0400;
const TIPG:  u32 = 0x0410;
const RDBAL: u32 = 0x2800;
const RDBAH: u32 = 0x2804;
const RDLEN: u32 = 0x2808;
const RDH:   u32 = 0x2810;
const RDT:   u32 = 0x2818;
const TDBAL: u32 = 0x3800;
const TDBAH: u32 = 0x3804;
const TDLEN: u32 = 0x3808;
const TDH:   u32 = 0x3810;
const TDT:   u32 = 0x3818;
const MTA:   u32 = 0x5200;
const RAL0:  u32 = 0x5400;
const RAH0:  u32 = 0x5404;
const EERD:  u32 = 0x0014;

// ── Ring sizes ────────────────────────────────────────────────────────────────

const RX_RING: usize = 32;
const TX_RING: usize = 32;
const RX_BUF:  usize = 2048;
const TX_BUF:  usize = 1514;

// ── RX descriptor (legacy, 16 bytes) ─────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RxDesc {
    addr:     u64,
    length:   u16,
    checksum: u16,
    status:   u8,
    errors:   u8,
    special:  u16,
}

const RX_DD: u8 = 1 << 0;

// ── TX descriptor (legacy, 16 bytes) ─────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TxDesc {
    addr:    u64,
    length:  u16,
    cso:     u8,
    cmd:     u8,
    status:  u8,
    css:     u8,
    special: u16,
}

const TX_EOP:  u8 = 1 << 0;
const TX_IFCS: u8 = 1 << 1;
const TX_RS:   u8 = 1 << 3;
const TX_DD:   u8 = 1 << 0;

// ── I/O port helpers ──────────────────────────────────────────────────────────

#[inline]
fn port_outl(port: u16, val: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack)); }
}

#[inline]
fn port_inl(port: u16) -> u32 {
    let v: u32;
    unsafe { asm!("in eax, dx", out("eax") v, in("dx") port, options(nomem, nostack)); }
    v
}

// ── Driver state ──────────────────────────────────────────────────────────────

pub struct E1000 {
    io_base:  u16,
    pub mac:  [u8; 6],
    rx_ring:  Box<[RxDesc; RX_RING]>,
    rx_bufs:  Box<[[u8; RX_BUF]; RX_RING]>,
    rx_tail:  usize,
    tx_ring:  Box<[TxDesc; TX_RING]>,
    tx_bufs:  Box<[[u8; TX_BUF]; TX_RING]>,
    tx_tail:  usize,
}

pub static mut DRIVER:  Option<E1000> = None;
pub static     PRESENT: AtomicBool    = AtomicBool::new(false);

// ── I/O port register accessors (IOADDR + IODATA) ────────────────────────────

impl E1000 {
    #[inline]
    fn r(&self, reg: u32) -> u32 {
        port_outl(self.io_base,     reg);
        port_inl (self.io_base + 4)
    }
    #[inline]
    fn w(&self, reg: u32, v: u32) {
        port_outl(self.io_base,     reg);
        port_outl(self.io_base + 4, v);
    }
}

// ── Physical address helper ───────────────────────────────────────────────────

fn virt_to_phys(virt: *const u8) -> u64 {
    let hhdm = crate::kernel::paging_allocator::get_hhdm_offset();
    if hhdm == 0 { virt as u64 } else { virt as u64 - hhdm }
}

// ── Detection & init ──────────────────────────────────────────────────────────

/// Detect any supported e1000 variant via PCI and bring up the driver.
/// Returns `true` on success.
pub unsafe fn init() -> bool {
    let sp = &crate::kernel::serial::SERIAL_PORT;

    let mut found_dev = None;
    'outer: for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let id = pci::read32(bus, dev, 0, 0x00);
            let vendor = id as u16;
            if vendor != VENDOR { continue; }
            let device = (id >> 16) as u16;
            if DEVICES.contains(&device) {
                found_dev = Some(pci::PciDevice { bus, dev, func: 0, vendor, device });
                break 'outer;
            }
        }
    }

    let pci_dev = match found_dev {
        Some(d) => d,
        None => {
            sp.write_str("[net] e1000 not found\n");
            return false;
        }
    };

    // BAR2 (offset 0x18) is the 8-byte I/O port BAR on 82540EM.
    // Fall back to BAR4 for variants that use a different slot.
    let io_base = match pci_dev.io_bar(2) {
        Some(b) if b > 0 => b,
        _ => match pci_dev.io_bar(4) {
            Some(b) if b > 0 => b,
            _ => {
                sp.write_str("[net] e1000 found but no I/O BAR — skipping\n");
                return false;
            }
        },
    };

    pci_dev.enable_bus_mastering();

    let mut nic = E1000 {
        io_base,
        mac:     [0; 6],
        rx_ring: Box::new([RxDesc::default(); RX_RING]),
        rx_bufs: Box::new([[0u8; RX_BUF];   RX_RING]),
        rx_tail: 0,
        tx_ring: Box::new([TxDesc::default(); TX_RING]),
        tx_bufs: Box::new([[0u8; TX_BUF];   TX_RING]),
        tx_tail: 0,
    };

    sp.write_str("[net] e1000 I/O base=0x");
    sp.write_hex(io_base as u32);
    sp.write_str("\n");

    // Device reset (RST = bit 26 of CTRL).
    let ctrl = nic.r(CTRL);
    nic.w(CTRL, ctrl | (1 << 26));
    for _ in 0..20_000u32 {
        unsafe { core::arch::asm!("pause", options(nostack, nomem)); }
    }

    // Mask all interrupts.
    nic.w(IMC, 0xFFFF_FFFF);

    nic.read_mac();

    for i in 0..128u32 {
        nic.w(MTA + i * 4, 0);
    }

    nic.setup_rx();
    nic.setup_tx();

    // Auto-speed detection + set link up.
    let ctrl = nic.r(CTRL);
    nic.w(CTRL, ctrl | (1 << 5) | (1 << 6)); // ASDE | SLU

    let mac = nic.mac;
    sp.write_str("[net] e1000 (0x");
    sp.write_hex(pci_dev.device as u32);
    sp.write_str(") MAC ");
    for i in 0..6 {
        if i > 0 { sp.write_str(":"); }
        sp.write_byte(b"0123456789ABCDEF"[(mac[i] >> 4) as usize]);
        sp.write_byte(b"0123456789ABCDEF"[(mac[i] & 0xF) as usize]);
    }
    sp.write_str("\n");

    unsafe { DRIVER = Some(nic); }
    PRESENT.store(true, Ordering::SeqCst);
    true
}

impl E1000 {
    // ── EEPROM ────────────────────────────────────────────────────────────────

    fn eeprom_read(&self, addr: u8) -> Option<u16> {
        self.w(EERD, ((addr as u32) << 8) | 1);
        // Spin until DONE (bit 4) with timeout — no infinite loop.
        for _ in 0..100_000u32 {
            let v = self.r(EERD);
            if v & (1 << 4) != 0 {
                return Some((v >> 16) as u16);
            }
            unsafe { core::arch::asm!("pause", options(nostack, nomem)); }
        }
        None
    }

    fn read_mac(&mut self) {
        if let (Some(w0), Some(w1), Some(w2)) = (
            self.eeprom_read(0),
            self.eeprom_read(1),
            self.eeprom_read(2),
        ) {
            let mac = [
                w0 as u8, (w0 >> 8) as u8,
                w1 as u8, (w1 >> 8) as u8,
                w2 as u8, (w2 >> 8) as u8,
            ];
            if mac != [0; 6] && mac != [0xFF; 6] {
                self.mac = mac;
                return;
            }
        }
        // Fall back to RAL0/RAH0 set by firmware.
        let ral = self.r(RAL0);
        let rah = self.r(RAH0);
        self.mac = [
            ral as u8, (ral >> 8) as u8, (ral >> 16) as u8, (ral >> 24) as u8,
            rah as u8, (rah >> 8) as u8,
        ];
    }

    // ── RX setup ─────────────────────────────────────────────────────────────

    fn setup_rx(&mut self) {
        for i in 0..RX_RING {
            let phys = virt_to_phys(self.rx_bufs[i].as_ptr());
            self.rx_ring[i].addr   = phys;
            self.rx_ring[i].status = 0;
        }

        let ring_phys = virt_to_phys(self.rx_ring.as_ptr() as *const u8);
        self.w(RDBAL, ring_phys as u32);
        self.w(RDBAH, (ring_phys >> 32) as u32);
        self.w(RDLEN, (RX_RING * 16) as u32);
        self.w(RDH,   0);
        self.rx_tail = RX_RING - 1;
        self.w(RDT,   self.rx_tail as u32);

        // RCTL: EN | BAM | BSIZE=2048 | SECRC
        self.w(RCTL, (1 << 1) | (1 << 15) | (0 << 16) | (1 << 26));
    }

    // ── TX setup ─────────────────────────────────────────────────────────────

    fn setup_tx(&mut self) {
        let ring_phys = virt_to_phys(self.tx_ring.as_ptr() as *const u8);
        self.w(TDBAL, ring_phys as u32);
        self.w(TDBAH, (ring_phys >> 32) as u32);
        self.w(TDLEN, (TX_RING * 16) as u32);
        self.w(TDH,   0);
        self.tx_tail = 0;
        self.w(TDT,   0);

        self.w(TIPG, 10 | (4 << 10) | (6 << 20));
        // TCTL: EN | PSP | CT=0x0F | COLD=0x40
        self.w(TCTL, (1 << 1) | (1 << 3) | (0x0F << 4) | (0x40 << 12));
    }

    // ── Send ─────────────────────────────────────────────────────────────────

    pub fn send(&mut self, frame: &[u8]) -> bool {
        if frame.len() > TX_BUF { return false; }

        let slot = self.tx_tail;
        let mut wait = 100_000u32;
        loop {
            let s = self.tx_ring[slot].status;
            if s & TX_DD != 0 || s == 0 { break; }
            wait -= 1;
            if wait == 0 { return false; }
            unsafe { core::arch::asm!("pause", options(nostack, nomem)); }
        }

        let len = frame.len();
        self.tx_bufs[slot][..len].copy_from_slice(frame);
        let buf_phys = virt_to_phys(self.tx_bufs[slot].as_ptr());
        self.tx_ring[slot].addr   = buf_phys;
        self.tx_ring[slot].length = len as u16;
        self.tx_ring[slot].cmd    = TX_EOP | TX_IFCS | TX_RS;
        self.tx_ring[slot].status = 0;

        self.tx_tail = (slot + 1) % TX_RING;
        self.w(TDT, self.tx_tail as u32);
        true
    }

    // ── Receive ───────────────────────────────────────────────────────────────

    pub fn recv(&mut self, out: &mut [u8]) -> usize {
        let next = (self.rx_tail + 1) % RX_RING;
        if self.rx_ring[next].status & RX_DD == 0 { return 0; }

        let len = self.rx_ring[next].length as usize;
        let n   = len.min(out.len());
        out[..n].copy_from_slice(&self.rx_bufs[next][..n]);

        self.rx_ring[next].status = 0;
        self.rx_tail = next;
        self.w(RDT, self.rx_tail as u32);
        n
    }
}
