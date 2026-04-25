//! Networking subsystem for OxideOS.
//!
//! Layer stack:
//!   pci.rs     — PCI bus enumeration (config-space port I/O)
//!   rtl8139.rs — RTL8139 Ethernet driver (QEMU)
//!   e1000.rs   — Intel e1000 driver (VirtualBox/VMware, I/O port)
//!   pcnet.rs   — AMD PCnet driver (VirtualBox PCnet-FAST III)
//!   stack.rs   — smoltcp integration (Interface + SocketSet)
//!   socket.rs  — per-process socket table + syscall implementations

pub mod pci;
pub mod rtl8139;
pub mod e1000;
pub mod pcnet;
pub mod stack;
pub mod socket;
pub mod dns;

/// Initialise the full networking subsystem.
/// Tries RTL8139 → e1000 → PCnet in order.
pub unsafe fn init() {
    pci::enumerate_to_serial();

    let found = unsafe { rtl8139::init() }
             || unsafe { e1000::init()  }
             || unsafe { pcnet::init()  };

    if !found {
        crate::kernel::serial::SERIAL_PORT.write_str("[net] No supported NIC found\n");
    }

    if found {
        unsafe { stack::init() };
    }
}

/// Drive the network stack (call every timer tick or GUI frame).
pub unsafe fn poll() {
    unsafe { stack::poll() };
}

/// Returns `true` if any network interface is up.
pub fn is_present() -> bool {
    use core::sync::atomic::Ordering;
    rtl8139::PRESENT.load(Ordering::Relaxed)
    || e1000::PRESENT.load(Ordering::Relaxed)
    || pcnet::PRESENT.load(Ordering::Relaxed)
}

/// Name of the active NIC for display purposes.
pub fn nic_name() -> &'static str {
    use core::sync::atomic::Ordering;
    if rtl8139::PRESENT.load(Ordering::Relaxed) { return "RTL8139"; }
    if e1000::PRESENT.load(Ordering::Relaxed)   { return "e1000";   }
    if pcnet::PRESENT.load(Ordering::Relaxed)   { return "PCnet";   }
    "None"
}

/// Resolve a hostname to an IPv4 address using the configured DNS server.
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
            if let Some(d) = &*core::ptr::addr_of!(rtl8139::DRIVER) {
                return Some(d.mac);
            }
        }
    }
    if e1000::PRESENT.load(Ordering::Relaxed) {
        unsafe {
            if let Some(d) = &*core::ptr::addr_of!(e1000::DRIVER) {
                return Some(d.mac);
            }
        }
    }
    if pcnet::PRESENT.load(Ordering::Relaxed) {
        unsafe {
            if let Some(d) = &*core::ptr::addr_of!(pcnet::DRIVER) {
                return Some(d.mac);
            }
        }
    }
    None
}
