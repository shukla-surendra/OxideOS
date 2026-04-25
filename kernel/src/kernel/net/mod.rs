//! Networking subsystem for OxideOS.
//!
//! Layer stack:
//!   pci.rs     — PCI bus enumeration (config-space port I/O)
//!   rtl8139.rs — RTL8139 Ethernet driver
//!   stack.rs   — smoltcp integration (Interface + SocketSet)
//!   socket.rs  — per-process socket table + syscall implementations

pub mod pci;
pub mod rtl8139;
pub mod e1000;
pub mod stack;
pub mod socket;
pub mod dns;

/// Initialise the full networking subsystem.
/// Tries RTL8139 first (QEMU default), then e1000 (VirtualBox default).
pub unsafe fn init() {
    // Dump all PCI devices to serial so it's easy to see what's present.
    pci::enumerate_to_serial();

    // 1. Try RTL8139 (QEMU -device rtl8139).
    let found = unsafe { rtl8139::init() }
             || unsafe { e1000::init() };   // 2. Try Intel e1000 (VirtualBox default)

    if !found {
        crate::kernel::serial::SERIAL_PORT.write_str("[net] No supported NIC found\n");
    }

    // 3. If any NIC came up, configure the IP stack.
    if found {
        unsafe { stack::init() };
    }
}

/// Drive the network stack.
/// Must be called periodically (e.g. every timer tick or every GUI frame).
pub unsafe fn poll() {
    unsafe { stack::poll() };
}

/// Returns `true` if a network interface is available.
pub fn is_present() -> bool {
    use core::sync::atomic::Ordering;
    rtl8139::PRESENT.load(Ordering::Relaxed)
    || e1000::PRESENT.load(Ordering::Relaxed)
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

/// Returns the NIC MAC address.
pub fn get_mac() -> Option<[u8; 6]> {
    use core::sync::atomic::Ordering;
    if rtl8139::PRESENT.load(Ordering::Relaxed) {
        unsafe {
            match &*core::ptr::addr_of!(rtl8139::DRIVER) {
                Some(d) => return Some(d.mac),
                None    => {}
            }
        }
    }
    if e1000::PRESENT.load(Ordering::Relaxed) {
        unsafe {
            match &*core::ptr::addr_of!(e1000::DRIVER) {
                Some(d) => return Some(d.mac),
                None    => {}
            }
        }
    }
    None
}
