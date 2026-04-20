//! Networking subsystem for OxideOS.
//!
//! Layer stack:
//!   pci.rs     — PCI bus enumeration (config-space port I/O)
//!   rtl8139.rs — RTL8139 Ethernet driver
//!   stack.rs   — smoltcp integration (Interface + SocketSet)
//!   socket.rs  — per-process socket table + syscall implementations

pub mod pci;
pub mod rtl8139;
pub mod stack;
pub mod socket;
pub mod dns;

/// Initialise the full networking subsystem.
/// Call this once during kernel boot after the heap is available.
pub unsafe fn init() {
    // Dump all PCI devices to serial so it's easy to see what's present.
    pci::enumerate_to_serial();

    // 1. Find and bring up the RTL8139.
    let found = rtl8139::init();

    // 2. If the NIC came up, configure the IP stack.
    if found {
        stack::init();
    }
}

/// Drive the network stack.
/// Must be called periodically (e.g. every timer tick or every GUI frame).
pub unsafe fn poll() {
    stack::poll();
}

/// Returns `true` if a network interface is available.
pub fn is_present() -> bool {
    rtl8139::PRESENT.load(core::sync::atomic::Ordering::Relaxed)
}

/// Resolve a hostname to an IPv4 address using the configured DNS server.
/// Returns `None` on failure or if the network is not up.
pub fn dns_resolve(hostname: &[u8]) -> Option<[u8; 4]> {
    dns::resolve(hostname)
}

/// Returns the current IP address (DHCP or static fallback).
pub fn get_ip() -> [u8; 4] {
    stack::get_ip()
}

/// Returns the NIC MAC address if the RTL8139 is present.
pub fn get_mac() -> Option<[u8; 6]> {
    if !is_present() { return None; }
    unsafe {
        let ptr = core::ptr::addr_of!(rtl8139::DRIVER);
        match &*ptr {
            Some(d) => Some(d.mac),
            None    => None,
        }
    }
}
