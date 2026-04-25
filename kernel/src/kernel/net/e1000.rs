//! Intel 82540EM (e1000) Ethernet driver.
//!
//! Supports VirtualBox "Intel PRO/1000 MT Desktop (82540EM)" — the default
//! VirtualBox NAT adapter.  Uses MMIO register access via BAR0.
//!
//! Also detected: 82545EM (0x100F), 82543GC (0x1004), 82541PI (0x107C).

extern crate alloc;

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};
use super::pci;

// ── Supported PCI device IDs (all vendor 0x8086) ─────────────────────────────

const VENDOR: u16 = 0x8086;
const DEVICES: &[u16] = &[
    0x100E, // 82540EM  — VirtualBox default
    0x100F, // 82545EM
    0x1004, // 82543GC
    0x107C, // 82541PI
    0x10D3, // 82574L
];

// ── MMIO register offsets ────────────────────────────────────────────────────

const CTRL:  u32 = 0x0000;
const ICR:   u32 = 0x00C0;
const IMS:   u32 = 0x00D0;
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
const MTA:   u32 = 0x5200; // multicast table array (128 DWORDs)
const RAL0:  u32 = 0x5400; // receive address low
const RAH0:  u32 = 0x5404; // receive address high + AV bit
const EERD:  u32 = 0x0014; // EEPROM read

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
    status:   u8,   // bit 0: DD (done), bit 1: EOP
    errors:   u8,
    special:  u16,
}

const RX_DD:  u8 = 1 << 0;

// ── TX descriptor (legacy, 16 bytes) ─────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TxDesc {
    addr:    u64,
    length:  u16,
    cso:     u8,
    cmd:     u8,    // bit 0: EOP, bit 1: IFCS, bit 3: RS
    status:  u8,    // bit 0: DD
    css:     u8,
    special: u16,
}

const TX_EOP:  u8 = 1 << 0;
const TX_IFCS: u8 = 1 << 1;
const TX_RS:   u8 = 1 << 3;
const TX_DD:   u8 = 1 << 0;

// ── Driver state ──────────────────────────────────────────────────────────────

pub struct E1000 {
    mmio:     u64, // kernel virtual address of MMIO region
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

// ── MMIO accessors ────────────────────────────────────────────────────────────

impl E1000 {
    #[inline]
    fn r(&self, reg: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.mmio + reg as u64) as *const u32) }
    }
    #[inline]
    fn w(&self, reg: u32, v: u32) {
        unsafe { core::ptr::write_volatile((self.mmio + reg as u64) as *mut u32, v) }
    }
}

// ── Detection & init ──────────────────────────────────────────────────────────

/// Detect any supported e1000 variant via PCI and bring up the driver.
/// Returns `true` on success.
pub unsafe fn init() -> bool {
    let sp = &crate::kernel::serial::SERIAL_PORT;

    // Try each supported device ID.
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

    // BAR0 is a 64-bit memory BAR. Read both halves.
    let bar0_lo = pci::read32(pci_dev.bus, pci_dev.dev, pci_dev.func, 0x10);
    let bar0_hi = pci::read32(pci_dev.bus, pci_dev.dev, pci_dev.func, 0x14);
    let phys = ((bar0_hi as u64) << 32) | (bar0_lo as u64 & !0xF);

    // Convert physical MMIO address to kernel virtual via HHDM.
    let hhdm = crate::kernel::paging_allocator::get_hhdm_offset();
    let mmio  = hhdm + phys;

    pci_dev.enable_bus_mastering();

    let mut nic = E1000 {
        mmio,
        mac:     [0; 6],
        rx_ring: Box::new([RxDesc::default(); RX_RING]),
        rx_bufs: Box::new([[0u8; RX_BUF];   RX_RING]),
        rx_tail: 0,
        tx_ring: Box::new([TxDesc::default(); TX_RING]),
        tx_bufs: Box::new([[0u8; TX_BUF];   TX_RING]),
        tx_tail: 0,
    };

    // Device reset (RST = bit 26 of CTRL).
    let ctrl = nic.r(CTRL);
    nic.w(CTRL, ctrl | (1 << 26));
    // Spin briefly for reset to complete.
    for _ in 0..20_000u32 {
        unsafe { core::arch::asm!("pause", options(nostack, nomem)); }
    }

    // Mask all interrupts.
    nic.w(IMC, 0xFFFF_FFFF);

    // Read MAC from EEPROM.
    nic.read_mac();

    // Clear multicast table.
    for i in 0..128u32 {
        nic.w(MTA + i * 4, 0);
    }

    // Set up descriptors.
    nic.setup_rx();
    nic.setup_tx();

    // Re-enable link and set auto-speed detection.
    let ctrl = nic.r(CTRL);
    nic.w(CTRL, ctrl | (1 << 5) | (1 << 6)); // ASDE | SLU

    let mac = nic.mac;
    sp.write_str("[net] e1000 (");
    sp.write_hex(pci_dev.device as u32);
    sp.write_str(") MMIO=0x");
    sp.write_hex((mmio >> 32) as u32);
    sp.write_hex(mmio as u32);
    sp.write_str(" MAC ");
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

    fn eeprom_read(&self, addr: u8) -> u16 {
        // Write start + address, spin until DONE (bit 4) set.
        self.w(EERD, ((addr as u32) << 8) | 1);
        loop {
            let v = self.r(EERD);
            if v & (1 << 4) != 0 {
                return (v >> 16) as u16;
            }
            unsafe { core::arch::asm!("pause", options(nostack, nomem)); }
        }
    }

    fn read_mac(&mut self) {
        // Try EEPROM first (words 0, 1, 2 → bytes 0-1, 2-3, 4-5).
        let w0 = self.eeprom_read(0);
        let w1 = self.eeprom_read(1);
        let w2 = self.eeprom_read(2);
        self.mac = [
            w0 as u8, (w0 >> 8) as u8,
            w1 as u8, (w1 >> 8) as u8,
            w2 as u8, (w2 >> 8) as u8,
        ];

        // If EEPROM gives all-zeros or all-ones, fall back to RAL0/RAH0.
        if self.mac == [0; 6] || self.mac == [0xFF; 6] {
            let ral = self.r(RAL0);
            let rah = self.r(RAH0);
            self.mac = [
                ral as u8, (ral >> 8) as u8, (ral >> 16) as u8, (ral >> 24) as u8,
                rah as u8, (rah >> 8) as u8,
            ];
        }
    }

    // ── RX setup ─────────────────────────────────────────────────────────────

    fn setup_rx(&mut self) {
        // Point each descriptor at its buffer.
        for i in 0..RX_RING {
            self.rx_ring[i].addr   = self.rx_bufs[i].as_ptr() as u64;
            self.rx_ring[i].status = 0;
        }

        let phys = self.rx_ring.as_ptr() as u64;
        self.w(RDBAL, phys as u32);
        self.w(RDBAH, (phys >> 32) as u32);
        self.w(RDLEN, (RX_RING * 16) as u32);
        self.w(RDH,   0);
        self.rx_tail = RX_RING - 1;
        self.w(RDT,   self.rx_tail as u32);

        // RCTL: EN | BAM (broadcast) | BSIZE=2048 | SECRC (strip FCS)
        self.w(RCTL, (1 << 1) | (1 << 15) | (0 << 16) | (1 << 26));
    }

    // ── TX setup ─────────────────────────────────────────────────────────────

    fn setup_tx(&mut self) {
        let phys = self.tx_ring.as_ptr() as u64;
        self.w(TDBAL, phys as u32);
        self.w(TDBAH, (phys >> 32) as u32);
        self.w(TDLEN, (TX_RING * 16) as u32);
        self.w(TDH,   0);
        self.tx_tail = 0;
        self.w(TDT,   0);

        // Standard inter-packet gap for 802.3: IPGT=10, IPGR1=4, IPGR2=6.
        self.w(TIPG, 10 | (4 << 10) | (6 << 20));

        // TCTL: EN | PSP | CT=0x0F | COLD=0x40 (full-duplex 802.3)
        self.w(TCTL, (1 << 1) | (1 << 3) | (0x0F << 4) | (0x40 << 12));
    }

    // ── Send ─────────────────────────────────────────────────────────────────

    pub fn send(&mut self, frame: &[u8]) -> bool {
        if frame.len() > TX_BUF { return false; }

        let slot = self.tx_tail;

        // Wait for slot to be free (DD set by hardware, or status==0 initially).
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
        self.tx_ring[slot].addr   = self.tx_bufs[slot].as_ptr() as u64;
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

        // Return descriptor to hardware.
        self.rx_ring[next].status = 0;
        self.rx_tail = next;
        self.w(RDT, self.rx_tail as u32);
        n
    }
}
