//! smoltcp network stack integration with DHCP client.
//!
//! On init: attempts DHCP (up to 300 polls ≈ a few seconds).
//! Falls back to static 10.0.2.15/24 if DHCP times out.
//!
//! Call `init()` once after the RTL8139 driver is up, then call `poll()`
//! every GUI frame / timer tick.

extern crate alloc;

use core::sync::atomic::Ordering;

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::dhcpv4;
use smoltcp::socket::tcp::{self, Socket as TcpSocket};
use smoltcp::socket::udp::{self, Socket as UdpSocket};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

use super::rtl8139;
use super::e1000;

// ── Resolved network configuration ────────────────────────────────────────

pub struct NetConfig {
    pub ip:         [u8; 4],
    pub prefix_len: u8,
    pub gateway:    [u8; 4],
    /// DNS server (default: QEMU slirp DNS at 10.0.2.3)
    pub dns:        [u8; 4],
    pub dhcp_ok:    bool,
}

pub static mut NET_CONFIG: NetConfig = NetConfig {
    ip:         [10, 0, 2, 15],
    prefix_len: 24,
    gateway:    [10, 0, 2, 2],
    dns:        [10, 0, 2, 3],
    dhcp_ok:    false,
};

// ── Global network state ───────────────────────────────────────────────────

pub struct NetState {
    pub iface:       Interface,
    pub sockets:     SocketSet<'static>,
    pub dhcp_handle: Option<SocketHandle>,
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
            // Try RTL8139 first, then e1000.
            let n = {
                let ptr = core::ptr::addr_of_mut!(rtl8139::DRIVER);
                match &mut *ptr { Some(nic) => nic.recv(&mut buf), None => 0 }
            };
            if n > 0 { n } else {
                let ptr = core::ptr::addr_of_mut!(e1000::DRIVER);
                match &mut *ptr { Some(nic) => nic.recv(&mut buf), None => 0 }
            }
        };
        if n == 0 { return None; }
        Some((RtlRxToken { data: buf, len: n }, RtlTxToken))
    }

    fn transmit(&mut self, _ts: Instant) -> Option<RtlTxToken> {
        let rtl_ok = rtl8139::PRESENT.load(Ordering::Relaxed);
        let e1k_ok = e1000::PRESENT.load(Ordering::Relaxed);
        if rtl_ok || e1k_ok { Some(RtlTxToken) } else { None }
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
            // Send via whichever driver is present.
            let ptr = core::ptr::addr_of_mut!(rtl8139::DRIVER);
            if let Some(nic) = &mut *ptr {
                nic.send(&buf[..len]);
            } else {
                let ptr = core::ptr::addr_of_mut!(e1000::DRIVER);
                if let Some(nic) = &mut *ptr {
                    nic.send(&buf[..len]);
                }
            }
        }
        result
    }
}

// ── Initialisation ─────────────────────────────────────────────────────────

pub unsafe fn init() {
    // Get MAC from whichever driver initialised successfully.
    let mac = unsafe {
        if rtl8139::PRESENT.load(Ordering::Relaxed) {
            let ptr = core::ptr::addr_of!(rtl8139::DRIVER);
            match &*ptr { Some(d) => d.mac, None => return }
        } else if e1000::PRESENT.load(Ordering::Relaxed) {
            let ptr = core::ptr::addr_of!(e1000::DRIVER);
            match &*ptr { Some(d) => d.mac, None => return }
        } else {
            return;
        }
    };

    let hw_addr = EthernetAddress(mac);
    let config  = Config::new(hw_addr.into());
    let now     = timestamp();

    let mut nic   = NicDevice;
    let mut iface = Interface::new(config, &mut nic, now);

    let mut sockets = SocketSet::new(alloc::vec![]);

    // Add DHCP socket — smoltcp will send DISCOVER automatically on first poll.
    let dhcp_socket  = dhcpv4::Socket::new();
    let dhcp_handle  = sockets.add(dhcp_socket);

    // Spin-poll until we get a DHCP lease or give up (≈300 poll cycles).
    let mut got_config = false;
    for _ in 0..300u32 {
        let now = timestamp();
        iface.poll(now, &mut nic, &mut sockets);

        let event = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll();
        if let Some(dhcpv4::Event::Configured(cfg)) = event {
            apply_dhcp_config(&mut iface, &cfg);
            got_config = true;
            break;
        }

        // ~1 ms busy wait so smoltcp sees time advancing between retransmits.
        for _ in 0..50_000u32 { core::hint::spin_loop(); }
    }

    if !got_config {
        // Static fallback — works fine for QEMU user-mode networking.
        let ip   = Ipv4Address::new(10, 0, 2, 15);
        let cidr = IpCidr::new(IpAddress::Ipv4(ip), 24);
        let gw   = Ipv4Address::new(10, 0, 2, 2);
        iface.update_ip_addrs(|a| { a.push(cidr).ok(); });
        iface.routes_mut().add_default_ipv4_route(gw).ok();
        unsafe { crate::kernel::serial::SERIAL_PORT
            .write_str("[net] DHCP timeout — static 10.0.2.15/24\n"); }
    }

    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        *ptr = Some(NetState { iface, sockets, dhcp_handle: Some(dhcp_handle) });
    }
}

/// Apply a DHCP lease to the interface and store it in NET_CONFIG.
fn apply_dhcp_config(iface: &mut Interface, cfg: &dhcpv4::Config<'_>) {
    // Update IP.
    let cidr: Ipv4Cidr = cfg.address;
    iface.update_ip_addrs(|addrs| {
        // Replace existing addresses.
        addrs.clear();
        addrs.push(IpCidr::Ipv4(cidr)).ok();
    });

    // Update gateway.
    if let Some(gw) = cfg.router {
        iface.routes_mut().add_default_ipv4_route(gw).ok();
        unsafe { (*core::ptr::addr_of_mut!(NET_CONFIG)).gateway = gw.0; }
    }

    // Store in NET_CONFIG.
    unsafe {
        let nc = &mut *core::ptr::addr_of_mut!(NET_CONFIG);
        nc.ip         = cidr.address().0;
        nc.prefix_len = cidr.prefix_len();
        nc.dhcp_ok    = true;

        // Use first DNS server if provided, else keep QEMU default 10.0.2.3.
        if let Some(dns) = cfg.dns_servers.first() {
            nc.dns = dns.0;
        }
    }

    // Print lease info.
    let ip     = unsafe { (*core::ptr::addr_of!(NET_CONFIG)).ip };
    let pfxlen = unsafe { (*core::ptr::addr_of!(NET_CONFIG)).prefix_len };
    unsafe {
        let s = &crate::kernel::serial::SERIAL_PORT;
        s.write_str("[net] DHCP lease — ");
        s.write_decimal(ip[0] as u32); s.write_str(".");
        s.write_decimal(ip[1] as u32); s.write_str(".");
        s.write_decimal(ip[2] as u32); s.write_str(".");
        s.write_decimal(ip[3] as u32); s.write_str("/");
        s.write_decimal(pfxlen as u32);
        s.write_str("\n");
    }
}

// ── Polling ────────────────────────────────────────────────────────────────

pub unsafe fn poll() {
    let ptr = core::ptr::addr_of_mut!(NET);
    if let Some(state) = &mut *ptr {
        let now = timestamp();
        let mut nic = NicDevice;
        state.iface.poll(now, &mut nic, &mut state.sockets);

        // Keep processing DHCP events (renewal, reconfiguration).
        if let Some(h) = state.dhcp_handle {
            let event = state.sockets.get_mut::<dhcpv4::Socket>(h).poll();
            match event {
                Some(dhcpv4::Event::Configured(cfg)) => {
                    apply_dhcp_config(&mut state.iface, &cfg);
                }
                Some(dhcpv4::Event::Deconfigured) => {
                    state.iface.update_ip_addrs(|a| a.clear());
                    (*core::ptr::addr_of_mut!(NET_CONFIG)).dhcp_ok = false;
                }
                None => {}
            }
        }
    }
}

pub fn timestamp() -> Instant {
    let ticks = unsafe { crate::kernel::timer::get_ticks() };
    Instant::from_millis((ticks * 10) as i64)
}

// ── Socket helpers (used by socket.rs and dns.rs) ─────────────────────────

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

/// Returns the current IP address (from DHCP or static fallback).
pub fn get_ip() -> [u8; 4] {
    unsafe { (*core::ptr::addr_of!(NET_CONFIG)).ip }
}

/// Returns the DNS server IP.
pub fn get_dns() -> [u8; 4] {
    unsafe { (*core::ptr::addr_of!(NET_CONFIG)).dns }
}
