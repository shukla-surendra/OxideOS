//! AMD PCnet-FAST III / PCnet-PCI II driver.
//!
//! Covers PCI 0x1022:0x2000 — the VirtualBox default "PCnet-FAST III" adapter.
//! Uses 32-bit DWIO mode (software style 2).  All register access via I/O ports.
//!
//! Port layout (BAR0, 32 bytes):
//!   io+0x00  APROM  — ethernet address ROM (read first 6 bytes for MAC)
//!   io+0x10  RDP32  — CSR data port (32-bit)
//!   io+0x14  RAP32  — register address port (32-bit)
//!   io+0x18  RESET  — read to soft-reset
//!   io+0x1C  BDP32  — BCR data port (32-bit)

extern crate alloc;

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};
use core::arch::asm;
use super::pci;

const VENDOR: u16 = 0x1022; // AMD
const DEVICE: u16 = 0x2000; // PCnet-PCI II / PCnet-FAST III

// ── I/O port helpers ──────────────────────────────────────────────────────────

#[inline] fn outl(port: u16, val: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack)); }
}
#[inline] fn inl(port: u16) -> u32 {
    let v: u32;
    unsafe { asm!("in eax, dx", out("eax") v, in("dx") port, options(nomem, nostack)); }
    v
}
#[inline] fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack)); }
    v
}

// ── CSR/BCR access (requires 32-bit / DWIO mode active first) ────────────────

fn csr_read(io: u16, idx: u32) -> u32 {
    outl(io + 0x14, idx);
    inl (io + 0x10)
}
fn csr_write(io: u16, idx: u32, val: u32) {
    outl(io + 0x14, idx);
    outl(io + 0x10, val);
}
fn bcr_write(io: u16, idx: u32, val: u32) {
    outl(io + 0x14, idx);
    outl(io + 0x1C, val);
}

// ── Ring sizes (must be power of 2, max 512) ──────────────────────────────────

const RX_N: usize = 8;   // 2^3
const TX_N: usize = 8;
const RX_LOG2: u8  = 3;
const TX_LOG2: u8  = 3;
const BUF_SZ: usize = 1536; // ≥ max Ethernet frame

// ── Descriptors (software style 2 = 32-bit, 16 bytes each) ───────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RxDesc {
    addr:    u32,   // buffer physical address
    blen:    i16,   // negative buffer size
    status:  u16,   // bit15=OWN(card), bit9=STP, bit8=ENP
    msg_len: u32,   // bits[11:0] = received length
    _res:    u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TxDesc {
    addr:   u32,
    blen:   i16,    // negative frame length
    status: u16,    // bit15=OWN, bit9=STP, bit8=ENP
    misc:   u32,
    _res:   u32,
}

const OWN:  u16 = 1 << 15;
const STP:  u16 = 1 << 9;
const ENP:  u16 = 1 << 8;
const ERR:  u16 = 1 << 14;

// ── Init block (software style 2) ─────────────────────────────────────────────

#[repr(C, packed)]
struct InitBlock {
    mode:    u16,
    rlen:    u8,    // log2(rx_n) << 4
    tlen:    u8,    // log2(tx_n) << 4
    mac:     [u8; 6],
    _res:    u16,
    ladrf:   [u32; 2], // logical address filter (all zeros = no multicast)
    rx_base: u32,
    tx_base: u32,
}

// ── Driver state ──────────────────────────────────────────────────────────────

pub struct PCnet {
    io:      u16,
    pub mac: [u8; 6],
    rx:      Box<[RxDesc; RX_N]>,
    rx_bufs: Box<[[u8; BUF_SZ]; RX_N]>,
    rx_idx:  usize,
    tx:      Box<[TxDesc; TX_N]>,
    tx_bufs: Box<[[u8; BUF_SZ]; TX_N]>,
    tx_idx:  usize,
    _init:   Box<InitBlock>, // kept alive for DMA lifetime
}

pub static mut DRIVER:  Option<PCnet> = None;
pub static     PRESENT: AtomicBool    = AtomicBool::new(false);

fn virt_to_phys(p: *const u8) -> u32 {
    let hhdm = crate::kernel::paging_allocator::get_hhdm_offset();
    let pa = if hhdm == 0 { p as u64 } else { p as u64 - hhdm };
    pa as u32 // PCnet only supports 32-bit DMA
}

pub unsafe fn init() -> bool {
    let sp = &crate::kernel::serial::SERIAL_PORT;

    let pci_dev = match pci::find_device(VENDOR, DEVICE) {
        Some(d) => d,
        None => {
            sp.write_str("[net] PCnet not found\n");
            return false;
        }
    };

    let io = match pci_dev.io_bar(0) {
        Some(b) if b > 0 => b,
        _ => {
            sp.write_str("[net] PCnet: no I/O BAR\n");
            return false;
        }
    };

    sp.write_str("[net] PCnet io=0x");
    sp.write_hex(io as u32);
    sp.write_str("\n");

    pci_dev.enable_bus_mastering();

    // 1. Read MAC from APROM before reset (bytes 0-5 are stable).
    let mac = [
        inb(io + 0), inb(io + 1), inb(io + 2),
        inb(io + 3), inb(io + 4), inb(io + 5),
    ];

    // 2. Software reset.
    inl(io + 0x18); // read RESET32 → triggers reset
    for _ in 0..10_000u32 {
        unsafe { asm!("pause", options(nomem, nostack)); }
    }

    // 3. Write to RDP32 to enter 32-bit (DWIO) mode.
    outl(io + 0x10, 0);

    // 4. BCR20: SWSTYLE=2, SSIZE32=1 → value 0x0102.
    bcr_write(io, 20, 0x0102);

    // 5. Build ring buffers.
    let mut rx = Box::new([RxDesc::default(); RX_N]);
    let rx_bufs: Box<[[u8; BUF_SZ]; RX_N]> = Box::new([[0u8; BUF_SZ]; RX_N]);
    for i in 0..RX_N {
        rx[i].addr   = virt_to_phys(rx_bufs[i].as_ptr());
        rx[i].blen   = -(BUF_SZ as i16);
        rx[i].status = OWN; // give to card
    }

    let mut tx = Box::new([TxDesc::default(); TX_N]);
    let tx_bufs: Box<[[u8; BUF_SZ]; TX_N]> = Box::new([[0u8; BUF_SZ]; TX_N]);

    // 6. Build init block.
    let init = Box::new(InitBlock {
        mode:    0x0000,
        rlen:    RX_LOG2 << 4,
        tlen:    TX_LOG2 << 4,
        mac,
        _res:    0,
        ladrf:   [0; 2],
        rx_base: virt_to_phys(rx.as_ptr() as *const u8),
        tx_base: virt_to_phys(tx.as_ptr() as *const u8),
    });

    let init_phys = virt_to_phys(&*init as *const InitBlock as *const u8);

    // 7. Write init block address to CSR1/CSR2.
    csr_write(io, 1, init_phys & 0xFFFF);
    csr_write(io, 2, init_phys >> 16);

    // 8. Set CSR0 INIT bit, wait for IDON.
    csr_write(io, 0, 0x0001); // INIT
    let mut ok = false;
    for _ in 0..500_000u32 {
        if csr_read(io, 0) & 0x0100 != 0 { ok = true; break; } // IDON
        unsafe { asm!("pause", options(nomem, nostack)); }
    }
    if !ok {
        sp.write_str("[net] PCnet: IDON timeout\n");
        return false;
    }

    // 9. Clear IDON, start (STRT).
    csr_write(io, 0, 0x0002 | 0x0040); // STRT | INEA (enable interrupts — safe even if not wired)

    sp.write_str("[net] PCnet MAC ");
    for i in 0..6 {
        if i > 0 { sp.write_str(":"); }
        sp.write_byte(b"0123456789ABCDEF"[(mac[i] >> 4) as usize]);
        sp.write_byte(b"0123456789ABCDEF"[(mac[i] & 0xF) as usize]);
    }
    sp.write_str("\n");

    unsafe {
        DRIVER = Some(PCnet {
            io, mac,
            rx, rx_bufs, rx_idx: 0,
            tx, tx_bufs, tx_idx: 0,
            _init: init,
        });
    }
    PRESENT.store(true, Ordering::SeqCst);
    true
}

impl PCnet {
    pub fn send(&mut self, frame: &[u8]) -> bool {
        if frame.len() > BUF_SZ { return false; }

        let slot = self.tx_idx;
        // If card still owns this descriptor, TX ring is full.
        if self.tx[slot].status & OWN != 0 { return false; }

        let len = frame.len();
        self.tx_bufs[slot][..len].copy_from_slice(frame);
        self.tx[slot].addr   = virt_to_phys(self.tx_bufs[slot].as_ptr());
        self.tx[slot].blen   = -(len as i16);
        self.tx[slot].misc   = 0;
        // STP | ENP | OWN  — single-buffer packet
        core::sync::atomic::fence(Ordering::SeqCst);
        self.tx[slot].status = OWN | STP | ENP;

        self.tx_idx = (slot + 1) % TX_N;

        // Kick TX: set TDMD (transmit demand, CSR0 bit 3).
        csr_write(self.io, 0, 0x0008 | 0x0002 | 0x0040);
        true
    }

    pub fn recv(&mut self, out: &mut [u8]) -> usize {
        let slot = self.rx_idx;
        let desc = &mut self.rx[slot];

        // Card still owns this descriptor — no packet yet.
        if desc.status & OWN != 0 { return 0; }
        // Packet with error — recycle.
        if desc.status & ERR != 0 {
            desc.status = OWN;
            self.rx_idx = (slot + 1) % RX_N;
            return 0;
        }

        let len = (desc.msg_len & 0x0FFF) as usize;
        let n   = len.min(out.len());
        out[..n].copy_from_slice(&self.rx_bufs[slot][..n]);

        // Return descriptor to card.
        desc.msg_len = 0;
        core::sync::atomic::fence(Ordering::SeqCst);
        desc.status = OWN;

        self.rx_idx = (slot + 1) % RX_N;
        n
    }
}
