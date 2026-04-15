//! smoltcp network stack integration.
//!
//! IP  : 10.0.2.15/24  (QEMU user-mode network default)
//! GW  : 10.0.2.2
//!
//! Call `init()` once after the RTL8139 driver is up, then call `poll()`
//! every GUI frame / timer tick.

extern crate alloc;

use core::sync::atomic::Ordering;

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::tcp::{self, Socket as TcpSocket};
use smoltcp::socket::udp::{self, Socket as UdpSocket};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use super::rtl8139;

// ── Global network state ───────────────────────────────────────────────────

pub struct NetState {
    pub iface:   Interface,
    pub sockets: SocketSet<'static>,
}

pub static mut NET: Option<NetState> = None;

// ── smoltcp Device implementation ──────────────────────────────────────────

pub struct NicDevice;

impl Device for NicDevice {
    type RxToken<'a> = RtlRxToken;
    type TxToken<'a> = RtlTxToken;

    fn receive(&mut self, _ts: Instant) -> Option<(RtlRxToken, RtlTxToken)> {
        let mut buf = [0u8; 1514];
        let n = unsafe {
            let ptr = core::ptr::addr_of_mut!(rtl8139::DRIVER);
            match &mut *ptr {
                Some(nic) => nic.recv(&mut buf),
                None      => 0,
            }
        };
        if n == 0 { return None; }
        Some((RtlRxToken { data: buf, len: n }, RtlTxToken))
    }

    fn transmit(&mut self, _ts: Instant) -> Option<RtlTxToken> {
        if rtl8139::PRESENT.load(Ordering::Relaxed) { Some(RtlTxToken) } else { None }
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = 1514;
        caps
    }
}

pub struct RtlRxToken { data: [u8; 1514], len: usize }
pub struct RtlTxToken;

impl RxToken for RtlRxToken {
    fn consume<R, F>(self, f: F) -> R where F: FnOnce(&mut [u8]) -> R {
        let mut buf = self.data;
        f(&mut buf[..self.len])
    }
}

impl TxToken for RtlTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R where F: FnOnce(&mut [u8]) -> R {
        let mut buf = [0u8; 1514];
        let result = f(&mut buf[..len]);
        unsafe {
            let ptr = core::ptr::addr_of_mut!(rtl8139::DRIVER);
            if let Some(nic) = &mut *ptr {
                nic.send(&buf[..len]);
            }
        }
        result
    }
}

// ── Initialisation ─────────────────────────────────────────────────────────

pub unsafe fn init() {
    if !rtl8139::PRESENT.load(Ordering::Relaxed) { return; }

    let mac = unsafe {
        let ptr = core::ptr::addr_of!(rtl8139::DRIVER);
        match &*ptr {
            Some(d) => d.mac,
            None    => return,
        }
    };

    let ip_addr = Ipv4Address::new(10, 0, 2, 15);
    let ip_cidr = IpCidr::new(IpAddress::Ipv4(ip_addr), 24);
    let gw_addr = Ipv4Address::new(10, 0, 2, 2);

    let hw_addr = EthernetAddress(mac);
    let config  = Config::new(hw_addr.into());
    let now     = timestamp();

    let mut nic    = NicDevice;
    let mut iface  = Interface::new(config, &mut nic, now);
    iface.update_ip_addrs(|addrs| { addrs.push(ip_cidr).ok(); });
    iface.routes_mut().add_default_ipv4_route(gw_addr).ok();

    let sockets = SocketSet::new(alloc::vec![]);

    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        *ptr = Some(NetState { iface, sockets });
    }

    crate::kernel::serial::SERIAL_PORT.write_str("[net] stack up — 10.0.2.15/24\n");
}

// ── Polling ────────────────────────────────────────────────────────────────

pub unsafe fn poll() {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        if let Some(state) = &mut *ptr {
            let now = timestamp();
            let mut nic = NicDevice;
            state.iface.poll(now, &mut nic, &mut state.sockets);
        }
    }
}

pub fn timestamp() -> Instant {
    let ticks = unsafe { crate::kernel::timer::get_ticks() };
    Instant::from_millis((ticks * 10) as i64)
}

// ── Socket helpers (used by socket.rs) ────────────────────────────────────

pub unsafe fn tcp_socket_new() -> Option<SocketHandle> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        let state = match &mut *ptr { Some(s) => s, None => return None };

        let rx = tcp::SocketBuffer::new(alloc::vec![0u8; 4096]);
        let tx = tcp::SocketBuffer::new(alloc::vec![0u8; 4096]);
        Some(state.sockets.add(TcpSocket::new(rx, tx)))
    }
}

pub unsafe fn udp_socket_new() -> Option<SocketHandle> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        let state = match &mut *ptr { Some(s) => s, None => return None };

        let rx = udp::PacketBuffer::new(
            alloc::vec![udp::PacketMetadata::EMPTY; 16],
            alloc::vec![0u8; 4096],
        );
        let tx = udp::PacketBuffer::new(
            alloc::vec![udp::PacketMetadata::EMPTY; 16],
            alloc::vec![0u8; 4096],
        );
        Some(state.sockets.add(UdpSocket::new(rx, tx)))
    }
}

pub unsafe fn socket_close(handle: SocketHandle) {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        if let Some(state) = &mut *ptr {
            state.sockets.remove(handle);
        }
    }
}
