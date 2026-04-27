//! RTL8139 Fast Ethernet driver.
//!
//! Supports QEMU emulation: `-netdev user,id=net0 -device rtl8139,netdev=net0`
//!
//! ## Memory layout
//!
//! - **RX ring** : 8 KB ring buffer at a physically-contiguous page.
//!   The chip writes packets here; we read them out and advance `rx_offset`.
//! - **TX buffers**: 4 × 1536-byte static buffers (one per descriptor slot).
//!   We write a packet into the next free slot, then start the descriptor.
//!
//! ## Register map (selected)
//!
//! | Offset | Name    | Description                     |
//! |--------|---------|---------------------------------|
//! | 0x00   | MAC0–5  | Hardware MAC address            |
//! | 0x37   | CR      | Command register                |
//! | 0x3C   | IMR     | Interrupt mask register         |
//! | 0x3E   | ISR     | Interrupt status register       |
//! | 0x44   | RCR     | Receive configuration register  |
//! | 0x40   | TCR     | Transmit configuration register |
//! | 0x30   | RBSTART | RX ring buffer start (phys addr)|
//! | 0x20–2B| TSAD0–3 | TX start address descriptors    |
//! | 0x10–1B| TSD0–3  | TX status descriptors           |
//! | 0x38   | CAPR    | Current address of packet read  |
//! | 0x3A   | CBR     | Current buffer address          |

extern crate alloc;

use alloc::boxed::Box;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use super::pci;

// ── RTL8139 register offsets ───────────────────────────────────────────────

const REG_MAC:     u16 = 0x00; // 6-byte MAC
const REG_MAR:     u16 = 0x08; // 8-byte multicast filter
const REG_TSD:     [u16; 4] = [0x10, 0x14, 0x18, 0x1C]; // TX status
const REG_TSAD:    [u16; 4] = [0x20, 0x24, 0x28, 0x2C]; // TX addr
const REG_RBSTART: u16 = 0x30; // RX ring start
const REG_ERBCR:   u16 = 0x34; // Early RX byte count (unused)
const REG_ERSR:    u16 = 0x36; // Early RX status  (unused)
const REG_CR:      u16 = 0x37; // Command register
const REG_CAPR:    u16 = 0x38; // Current addr of pkt read
const REG_CBR:     u16 = 0x3A; // Current buffer address
const REG_IMR:     u16 = 0x3C; // Interrupt mask
const REG_ISR:     u16 = 0x3E; // Interrupt status
const REG_TCR:     u16 = 0x40; // TX configuration
const REG_RCR:     u16 = 0x44; // RX configuration
const REG_TCTR:    u16 = 0x48; // Timer count (unused)
const REG_CONFIG1: u16 = 0x52;

// Command register bits
const CR_RESET:    u8 = 0x10;
const CR_RE:       u8 = 0x08; // Receiver enable
const CR_TE:       u8 = 0x04; // Transmitter enable
const CR_BUFE:     u8 = 0x01; // Buffer empty

// ISR bits
const ISR_ROK:  u16 = 0x0001; // RX OK
const ISR_TOK:  u16 = 0x0004; // TX OK
const ISR_TER:  u16 = 0x0008; // TX error
const ISR_RER:  u16 = 0x0002; // RX error

// RCR bits
const RCR_AAP:  u32 = 1 << 0; // Accept all packets (promiscuous)
const RCR_APM:  u32 = 1 << 1; // Accept physical match
const RCR_AM:   u32 = 1 << 2; // Accept multicast
const RCR_AB:   u32 = 1 << 3; // Accept broadcast
const RCR_WRAP: u32 = 1 << 7; // Wrap (don't stop at ring end)
// RBLEN: ring size bits [12:11]
// 00 = 8K+16, 01 = 16K+16, 10 = 32K+16, 11 = 64K+16
const RCR_RBLEN_8K:  u32 = 0b00 << 11;

// TX status bits
const TSD_OWN:  u32 = 1 << 13; // DMA complete — packet is ours to reuse
const TSD_TABT: u32 = 1 << 30; // TX abort
const TSD_TOK:  u32 = 1 << 15; // TX OK

// ── RX ring ────────────────────────────────────────────────────────────────

/// Size of the RX ring buffer (8 KB + 16 bytes overflow area).
const RX_BUF_SIZE: usize = 8192 + 16 + 1500;

/// TX buffer size per slot (1 full Ethernet frame).
const TX_BUF_SIZE: usize = 1536;
const TX_DESC_COUNT: usize = 4;

// ── Driver state ────────────────────────────────────────────────────────────

pub struct Rtl8139 {
    /// I/O base port for the card's registers.
    pub io_base: u16,
    /// Hardware MAC address (6 bytes).
    pub mac: [u8; 6],
    /// Receive ring buffer (allocated from the bump allocator).
    rx_buf: Box<[u8; RX_BUF_SIZE]>,
    /// Offset into rx_buf of the next packet to read.
    rx_offset: u16,
    /// Four TX frame buffers.
    tx_bufs: Box<[[u8; TX_BUF_SIZE]; TX_DESC_COUNT]>,
    /// Next TX descriptor slot (0–3, cycles).
    tx_slot: usize,
}

/// Global driver instance; `None` means the card was not found.
pub static mut DRIVER: Option<Rtl8139> = None;
pub static PRESENT: AtomicBool = AtomicBool::new(false);

// ── Port helpers ────────────────────────────────────────────────────────────

fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack)); }
    v
}
fn inw(port: u16) -> u16 {
    let v: u16;
    unsafe { asm!("in ax, dx", out("ax") v, in("dx") port, options(nomem, nostack)); }
    v
}
fn inl(port: u16) -> u32 {
    let v: u32;
    unsafe { asm!("in eax, dx", out("eax") v, in("dx") port, options(nomem, nostack)); }
    v
}
fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack)); }
}
fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack)); }
}
fn outl(port: u16, val: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack)); }
}

// ── NIC detection & init ────────────────────────────────────────────────────

/// Detect the RTL8139 via PCI and initialise the driver.
/// Returns `true` if the card was found and brought up.
pub unsafe fn init() -> bool {
    // PCI vendor 0x10EC = Realtek, device 0x8139 = RTL8139
    let pci_dev = match pci::find_device(0x10EC, 0x8139) {
        Some(d) => d,
        None    => {
            crate::kernel::serial::SERIAL_PORT.write_str("[net] RTL8139 not found\n");
            return false;
        }
    };

    let io_base = match pci_dev.io_bar(0) {
        Some(b) => b,
        None    => {
            crate::kernel::serial::SERIAL_PORT.write_str("[net] RTL8139: no I/O BAR\n");
            return false;
        }
    };

    pci_dev.enable_bus_mastering();

    // Allocate RX ring and TX buffers.
    let rx_buf  = Box::new([0u8; RX_BUF_SIZE]);
    let tx_bufs = Box::new([[0u8; TX_BUF_SIZE]; TX_DESC_COUNT]);

    let mut nic = Rtl8139 {
        io_base,
        mac:       [0; 6],
        rx_buf,
        rx_offset: 0,
        tx_bufs,
        tx_slot:   0,
    };

    // Bring up the NIC.
    nic.reset();
    nic.read_mac();
    nic.setup_rx();
    nic.setup_tx();
    nic.enable_interrupts();

    let mac = nic.mac;
    let sp  = &crate::kernel::serial::SERIAL_PORT;
    sp.write_str("[net] RTL8139 at I/O 0x");
    sp.write_hex(io_base as u32);
    sp.write_str(" MAC ");
    for i in 0..6 {
        if i > 0 { sp.write_str(":"); }
        let byte_str = [
            b"0123456789ABCDEF"[(mac[i] >> 4) as usize],
            b"0123456789ABCDEF"[(mac[i] & 0xF) as usize],
        ];
        for &b in &byte_str { sp.write_byte(b); }
    }
    sp.write_str("\n");

    DRIVER  = Some(nic);
    PRESENT.store(true, Ordering::SeqCst);
    true
}

impl Rtl8139 {
    // ── Hardware bring-up ──────────────────────────────────────────────────

    fn reset(&self) {
        // Power on.
        outb(self.io_base + REG_CONFIG1, 0x00);

        // Software reset.
        outb(self.io_base + REG_CR, CR_RESET);
        // Wait for RST bit to clear (≤ a few µs on real hardware).
        let mut timeout = 100_000u32;
        while inb(self.io_base + REG_CR) & CR_RESET != 0 {
            timeout -= 1;
            if timeout == 0 { break; }
        }
    }

    fn read_mac(&mut self) {
        for i in 0..6 {
            self.mac[i] = inb(self.io_base + REG_MAC + i as u16);
        }
    }

    fn setup_rx(&mut self) {
        // Give the chip the physical address of our RX ring.
        // In our identity-mapped kernel the virtual == physical address.
        let phys = self.rx_buf.as_ptr() as u32;
        outl(self.io_base + REG_RBSTART, phys);

        // Accept broadcast + physical match, wrap on overflow, 8 KB ring.
        outl(self.io_base + REG_RCR, RCR_AB | RCR_APM | RCR_WRAP | RCR_RBLEN_8K);
    }

    fn setup_tx(&self) {
        // Enable TX & RX.
        outb(self.io_base + REG_CR, CR_TE | CR_RE);

        // No TX padding / retries (default TCR is fine).
        outl(self.io_base + REG_TCR, 0x0000_0600);
    }

    fn enable_interrupts(&self) {
        // Ack any stale interrupts.
        outw(self.io_base + REG_ISR, 0xFFFF);
        // Unmask RX OK, TX OK, TX error, RX error.
        outw(self.io_base + REG_IMR, ISR_ROK | ISR_TOK | ISR_TER | ISR_RER);
    }

    // ── Transmit ───────────────────────────────────────────────────────────

    /// Copy `frame` into the next TX descriptor slot and start the DMA.
    /// Returns `true` on success, `false` if the frame is too large or
    /// the current slot is still busy.
    pub fn send(&mut self, frame: &[u8]) -> bool {
        if frame.len() > TX_BUF_SIZE { return false; }

        let slot = self.tx_slot;

        // Wait until the DMA engine has finished with this slot (OWN bit set).
        let tsd_port = self.io_base + REG_TSD[slot];
        let mut wait = 50_000u32;
        loop {
            let status = inl(tsd_port);
            // When TSD_OWN is set the NIC is done; slot 0 starts as 0 (free).
            if status & TSD_OWN != 0 || status == 0 { break; }
            wait -= 1;
            if wait == 0 { return false; }
        }

        // Copy frame data into the TX buffer.
        let len = frame.len();
        self.tx_bufs[slot][..len].copy_from_slice(frame);

        // Give the physical address of the buffer to the chip.
        let phys = self.tx_bufs[slot].as_ptr() as u32;
        outl(self.io_base + REG_TSAD[slot], phys);

        // Write TX status: set size (bits [12:0]), keep OWN=0 to hand off.
        outl(self.io_base + REG_TSD[slot], len as u32 & 0x1FFF);

        self.tx_slot = (slot + 1) % TX_DESC_COUNT;
        true
    }

    // ── Receive ────────────────────────────────────────────────────────────

    /// If a packet is available in the RX ring, copy it into `out` and
    /// return its length.  Returns 0 if no packet is ready.
    pub fn recv(&mut self, out: &mut [u8]) -> usize {
        // Check whether the buffer is empty.
        if inb(self.io_base + REG_CR) & CR_BUFE != 0 {
            return 0;
        }

        let offset = self.rx_offset as usize;

        // Each packet in the ring is preceded by a 4-byte header:
        //   [15:0]  RX status   (bit 0 = ROK)
        //   [31:16] packet len  (includes 4-byte Ethernet CRC)
        let hdr_lo = self.rx_buf[offset % RX_BUF_SIZE] as u16
                   | (self.rx_buf[(offset + 1) % RX_BUF_SIZE] as u16) << 8;
        let pkt_len = (self.rx_buf[(offset + 2) % RX_BUF_SIZE] as u16
                     | (self.rx_buf[(offset + 3) % RX_BUF_SIZE] as u16) << 8) as usize;

        // Status bit 0 must be set (ROK) and length sanity check.
        if hdr_lo & 1 == 0 || pkt_len < 4 || pkt_len > 1514 + 4 {
            // Bad packet — advance past it so we don't stall.
            let skip = ((4 + 60 + 3) & !3) as u16; // min Ethernet frame, aligned
            self.rx_offset = (self.rx_offset.wrapping_add(skip)) % 8192;
            outw(self.io_base + REG_CAPR, self.rx_offset.wrapping_sub(16));
            return 0;
        }

        let data_len = pkt_len - 4; // strip the 4-byte Ethernet CRC
        let copy_len = data_len.min(out.len());

        for i in 0..copy_len {
            out[i] = self.rx_buf[(offset + 4 + i) % RX_BUF_SIZE];
        }

        // Advance rx_offset past this packet (header + payload + CRC, aligned to 4).
        let advance = ((4 + pkt_len + 3) & !3) as u16;
        self.rx_offset = self.rx_offset.wrapping_add(advance) % 8192;
        // CAPR is written -16 bytes per the datasheet.
        outw(self.io_base + REG_CAPR, self.rx_offset.wrapping_sub(16));

        // Clear RX OK interrupt.
        outw(self.io_base + REG_ISR, ISR_ROK);

        copy_len
    }
}
