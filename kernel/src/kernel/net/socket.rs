//! Per-process socket table and socket syscall implementations.
//!
//! Socket FDs start at 200 to avoid colliding with file FDs (0–31).
//!
//! Syscall numbers:
//!   Socket      = 100
//!   Connect     = 102
//!   Send        = 105
//!   Recv        = 106
//!   CloseSocket = 107

extern crate alloc;

use smoltcp::iface::SocketHandle;
use smoltcp::socket::tcp::Socket as TcpSocket;
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

use super::stack;

// ── Socket types ───────────────────────────────────────────────────────────

pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM:  u32 = 2;
pub const AF_INET:     u32 = 2;

// ── Socket table ───────────────────────────────────────────────────────────

const MAX_SOCKETS: usize = 16;

pub struct SocketEntry {
    pub handle:    SocketHandle,
    pub sock_type: u32,
}

static mut SOCK_TABLE: [Option<SocketEntry>; MAX_SOCKETS] = [
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
];

fn alloc_slot() -> Option<usize> {
    unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        table.iter().position(|s| s.is_none())
    }
}

fn slot_from_fd(sfd: i64) -> Option<usize> {
    let idx = sfd.wrapping_sub(200) as usize;
    if idx >= MAX_SOCKETS { return None; }
    unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        if table[idx].is_some() { Some(idx) } else { None }
    }
}

// ── sockaddr_in layout (16 bytes, C ABI) ──────────────────────────────────
//   u16 sin_family   (AF_INET = 2, little-endian)
//   u16 sin_port     (big-endian)
//   u8  sin_addr[4]  (big-endian IPv4)
//   u8  _pad[8]

fn parse_sockaddr(buf: &[u8]) -> Option<IpEndpoint> {
    if buf.len() < 8 { return None; }
    let family = u16::from_le_bytes([buf[0], buf[1]]);
    if family as u32 != AF_INET { return None; }
    let port = u16::from_be_bytes([buf[2], buf[3]]);
    let ip   = Ipv4Address::new(buf[4], buf[5], buf[6], buf[7]);
    Some(IpEndpoint::new(IpAddress::Ipv4(ip), port))
}

// ── Syscall implementations ────────────────────────────────────────────────

pub unsafe fn sys_socket(domain: u32, sock_type: u32, _proto: u32) -> i64 {
    if domain != AF_INET { return -22; }

    let slot = match alloc_slot() { Some(s) => s, None => return -24 };

    let handle = match sock_type {
        SOCK_STREAM => match unsafe { stack::tcp_socket_new() } {
            Some(h) => h, None => return -12,
        },
        SOCK_DGRAM => match unsafe { stack::udp_socket_new() } {
            Some(h) => h, None => return -12,
        },
        _ => return -22,
    };

    unsafe {
        let table = &mut *core::ptr::addr_of_mut!(SOCK_TABLE);
        table[slot] = Some(SocketEntry { handle, sock_type });
    }
    (slot as i64) + 200
}

pub unsafe fn sys_connect(sfd: i64, addr_ptr: *const u8, addr_len: usize) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let sock_type = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        table[slot].as_ref().map(|e| e.sock_type).unwrap_or(0)
    };
    if sock_type != SOCK_STREAM { return -38; }

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        table[slot].as_ref().unwrap().handle
    };

    let addr_buf = unsafe { core::slice::from_raw_parts(addr_ptr, addr_len) };
    let endpoint = match parse_sockaddr(addr_buf) { Some(e) => e, None => return -22 };

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<TcpSocket>(handle);
        let ctx     = state.iface.context();
        // Use slot as ephemeral port to avoid collision
        match sock.connect(ctx, endpoint, 49152u16 + slot as u16) {
            Ok(_)  => 0,
            Err(_) => -111,
        }
    }
}

pub unsafe fn sys_send(sfd: i64, buf_ptr: *const u8, len: usize, _flags: u32) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_STREAM => e.handle,
            _ => return -38,
        }
    };

    let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<TcpSocket>(handle);
        if !sock.can_send() { return -11; }
        match sock.send_slice(buf) {
            Ok(n)  => n as i64,
            Err(_) => -32,
        }
    }
}

pub unsafe fn sys_recv(sfd: i64, buf_ptr: *mut u8, len: usize, _flags: u32) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_STREAM => e.handle,
            _ => return -38,
        }
    };

    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<TcpSocket>(handle);
        if !sock.can_recv() {
            if !sock.is_open() { return 0; } // EOF
            return -11; // EAGAIN
        }
        match sock.recv_slice(buf) {
            Ok(n)  => n as i64,
            Err(_) => -5,
        }
    }
}

/// Returns `true` once the TCP handshake is complete (socket can send data).
/// Used by the kernel-side connectivity probe — does not consume or send any data.
pub unsafe fn tcp_is_connected(sfd: i64) -> bool {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return false };

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_STREAM => e.handle,
            _ => return false,
        }
    };

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return false };
        let sock    = state.sockets.get_mut::<TcpSocket>(handle);
        sock.can_send()
    }
}

pub unsafe fn sys_close_socket(sfd: i64) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        table[slot].as_ref().unwrap().handle
    };

    unsafe { stack::socket_close(handle); }

    unsafe {
        let table = &mut *core::ptr::addr_of_mut!(SOCK_TABLE);
        table[slot] = None;
    }
    0
}
