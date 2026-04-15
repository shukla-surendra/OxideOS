//! Per-process socket table and socket syscall implementations.
//!
//! Socket FDs start at 200 to avoid colliding with file FDs (0–31).
//!
//! Syscall numbers:
//!   Socket      = 100
//!   Bind        = 101
//!   Connect     = 102
//!   Listen      = 103
//!   Accept      = 104
//!   Send        = 105
//!   Recv        = 106
//!   CloseSocket = 107
//!   Sendto      = 108
//!   Recvfrom    = 109

extern crate alloc;

use smoltcp::iface::SocketHandle;
use smoltcp::socket::tcp::Socket as TcpSocket;
use smoltcp::socket::udp::Socket as UdpSocket;
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
    /// For TCP listeners: the local port being listened on.
    pub listen_port: u16,
    /// True if this is a passive (listening) socket.
    pub listening: bool,
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
        table[slot] = Some(SocketEntry { handle, sock_type, listen_port: 0, listening: false });
    }
    (slot as i64) + 200
}

/// Bind a socket to a local address/port.
pub unsafe fn sys_bind(sfd: i64, addr_ptr: *const u8, addr_len: usize) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };
    let addr_buf = unsafe { core::slice::from_raw_parts(addr_ptr, addr_len) };
    let endpoint = match parse_sockaddr(addr_buf) { Some(e) => e, None => return -22 };

    let sock_type = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        table[slot].as_ref().map(|e| e.sock_type).unwrap_or(0)
    };

    match sock_type {
        SOCK_STREAM => {
            // For TCP, we just record the local port; listen() calls smoltcp listen.
            unsafe {
                let table = &mut *core::ptr::addr_of_mut!(SOCK_TABLE);
                if let Some(e) = &mut table[slot] {
                    e.listen_port = endpoint.port;
                }
            }
            0
        }
        SOCK_DGRAM => {
            // For UDP, bind the socket to the local endpoint immediately.
            unsafe {
                let net_ptr = core::ptr::addr_of_mut!(stack::NET);
                let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
                let handle  = { let table = &*core::ptr::addr_of!(SOCK_TABLE);
                                table[slot].as_ref().unwrap().handle };
                let sock    = state.sockets.get_mut::<UdpSocket>(handle);
                match sock.bind(endpoint) {
                    Ok(_)  => 0,
                    Err(_) => -98, // EADDRINUSE
                }
            }
        }
        _ => -22,
    }
}

/// Put a TCP socket into passive listen mode.
pub unsafe fn sys_listen(sfd: i64, _backlog: i32) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let (handle, port) = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_STREAM => (e.handle, e.listen_port),
            _ => return -38, // ENOTSOCK
        }
    };
    if port == 0 { return -22; } // must call bind first

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<TcpSocket>(handle);
        match sock.listen(port) {
            Ok(_)  => {
                let table = &mut *core::ptr::addr_of_mut!(SOCK_TABLE);
                if let Some(e) = &mut table[slot] { e.listening = true; }
                0
            }
            Err(_) => -98,
        }
    }
}

/// Accept an incoming connection on a listening TCP socket.
///
/// Returns a new socket fd on success, -11 (EAGAIN) if no connection is
/// ready yet, or a negative error code.
pub unsafe fn sys_accept(sfd: i64) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    // Check there is an incoming connection.
    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_STREAM && e.listening => e.handle,
            _ => return -22,
        }
    };

    let is_active = unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<TcpSocket>(handle);
        sock.is_active()
    };
    if !is_active { return -11; } // EAGAIN — no connection yet

    // Allocate a new socket slot for the accepted connection.
    // The accepted socket shares the same smoltcp handle for now (single-connection server).
    let new_slot = match alloc_slot() { Some(s) => s, None => return -24 };

    // Create a new TCP socket for future incoming connections (re-arm the listener).
    let new_handle = match unsafe { stack::tcp_socket_new() } {
        Some(h) => h,
        None    => return -12,
    };

    let port = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        table[slot].as_ref().map(|e| e.listen_port).unwrap_or(0)
    };

    // Replace the listener's handle with the new socket (re-arm for next connection).
    // The accepted connection keeps the old handle.
    let old_handle = unsafe {
        let table = &mut *core::ptr::addr_of_mut!(SOCK_TABLE);
        let entry  = table[slot].as_mut().unwrap();
        let old    = entry.handle;
        entry.handle      = new_handle;
        entry.listen_port = port;
        // Re-arm the new handle for listening.
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let _ = state.sockets.get_mut::<TcpSocket>(new_handle).listen(port);
        old
    };

    unsafe {
        let table = &mut *core::ptr::addr_of_mut!(SOCK_TABLE);
        table[new_slot] = Some(SocketEntry {
            handle:      old_handle,
            sock_type:   SOCK_STREAM,
            listen_port: 0,
            listening:   false, // accepted socket is not a listener
        });
    }
    (new_slot as i64) + 200
}

/// Send a datagram to a specific address (UDP).
pub unsafe fn sys_sendto(
    sfd: i64, buf_ptr: *const u8, len: usize,
    _flags: u32, addr_ptr: *const u8, addr_len: usize,
) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_DGRAM => e.handle,
            _ => return -38,
        }
    };

    let buf      = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
    let addr_buf = unsafe { core::slice::from_raw_parts(addr_ptr, addr_len) };
    let endpoint = match parse_sockaddr(addr_buf) { Some(e) => e, None => return -22 };

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<UdpSocket>(handle);
        match sock.send_slice(buf, endpoint) {
            Ok(_)  => len as i64,
            Err(_) => -11,
        }
    }
}

/// Receive a datagram from any source (UDP).
/// `addr_ptr` and `addr_len_ptr` are optional out-params for the source address.
pub unsafe fn sys_recvfrom(
    sfd: i64, buf_ptr: *mut u8, len: usize,
    _flags: u32, addr_ptr: *mut u8, addr_len_ptr: *mut u32,
) -> i64 {
    let slot = match slot_from_fd(sfd) { Some(s) => s, None => return -9 };

    let handle = unsafe {
        let table = &*core::ptr::addr_of!(SOCK_TABLE);
        match table[slot].as_ref() {
            Some(e) if e.sock_type == SOCK_DGRAM => e.handle,
            _ => return -38,
        }
    };

    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };

    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(stack::NET);
        let state   = match &mut *net_ptr { Some(s) => s, None => return -100 };
        let sock    = state.sockets.get_mut::<UdpSocket>(handle);
        if !sock.can_recv() { return -11; } // EAGAIN
        match sock.recv_slice(buf) {
            Ok((n, meta)) => {
                // Write source address into addr_ptr if provided.
                if !addr_ptr.is_null() && !addr_len_ptr.is_null() {
                    let out = core::slice::from_raw_parts_mut(addr_ptr, 16);
                    out[0] = 2; out[1] = 0; // AF_INET little-endian
                    let port = meta.endpoint.port;
                    out[2] = (port >> 8) as u8;
                    out[3] = port as u8;
                    if let IpAddress::Ipv4(ip4) = meta.endpoint.addr {
                        out[4..8].copy_from_slice(&ip4.0);
                    }
                    *addr_len_ptr = 16;
                }
                n as i64
            }
            Err(_) => -5,
        }
    }
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
