//! Non-blocking DNS A-record resolver.
//!
//! `resolve(hostname)` does ONE smoltcp poll per call and returns:
//!   >  0  — packed IPv4 (little-endian u32 cast to i64)
//!   -6    — EAGAIN: no response yet, call again after sleeping
//!  -105   — ENONET: failed / timed out
//!
//! Callers (the dns_resolve syscall handler + userspace loop) must retry
//! with a short sleep between calls so the scheduler can run other tasks.
//! This prevents the DNS query from blocking the entire kernel.

extern crate alloc;

use smoltcp::iface::SocketHandle;
use smoltcp::socket::udp::{self, Socket as UdpSocket};
use smoltcp::wire::{IpEndpoint, IpAddress, Ipv4Address};

use super::stack::{self, NET};

// ── Public API ─────────────────────────────────────────────────────────────────

/// Poll the DNS resolver once.  Returns a packed IPv4, -6 (EAGAIN), or -105.
pub fn resolve(hostname: &[u8]) -> i64 {
    if hostname.is_empty() || hostname.len() > 253 { return -105; }

    // Fast path: dotted-decimal IP requires no query.
    if let Some(ip) = parse_ipv4(hostname) {
        return i64::from(u32::from_le_bytes(ip));
    }

    let dns_ip = stack::get_dns();
    unsafe { poll_once(hostname, dns_ip) }
}

// ── Global DNS query state ─────────────────────────────────────────────────────
//
// Only one DNS query can be in flight at a time.  A new hostname cancels any
// previous query automatically.

const DNS_PORT: u16 = 53;
const MAX_POLLS: u32 = 300; // ~3 s at 10 ms per poll cycle in userspace

struct GlobalDns {
    active:       bool,
    handle:       Option<SocketHandle>,
    hostname:     [u8; 253],
    hostname_len: usize,
    polls:        u32,
}

// SAFETY: single-CPU kernel, no concurrent access.
static mut GDNS: GlobalDns = GlobalDns {
    active:       false,
    handle:       None,
    hostname:     [0u8; 253],
    hostname_len: 0,
    polls:        0,
};

unsafe fn poll_once(hostname: &[u8], dns_ip: [u8; 4]) -> i64 {
    unsafe {
        let net_ptr = core::ptr::addr_of_mut!(NET);
        let state = match &mut *net_ptr { Some(s) => s, None => return -105 };
        let gdns  = &raw mut GDNS;

        // ── Start or restart query if hostname changed ─────────────────────────
        let hlen = hostname.len();
        let same_host = (*gdns).active
            && (*gdns).hostname_len == hlen
            && {
                let stored = core::slice::from_raw_parts(
                    core::ptr::addr_of!((*gdns).hostname) as *const u8, hlen,
                );
                stored == hostname
            };

        if !same_host {
            // Drop any previous socket.
            if let Some(h) = (*gdns).handle.take() {
                state.sockets.remove(h);
            }

            // Allocate a fresh UDP socket.
            let rx = udp::PacketBuffer::new(
                alloc::vec![udp::PacketMetadata::EMPTY; 4],
                alloc::vec![0u8; 512],
            );
            let tx = udp::PacketBuffer::new(
                alloc::vec![udp::PacketMetadata::EMPTY; 4],
                alloc::vec![0u8; 512],
            );
            let mut sock = UdpSocket::new(rx, tx);

            // Ephemeral source port derived from timestamp to avoid rebind races.
            let local_port = 49152 + (stack::timestamp().millis() as u16 & 0x3FFF);
            let local_ep = IpEndpoint::new(
                IpAddress::Ipv4(Ipv4Address::UNSPECIFIED), local_port,
            );
            if sock.bind(local_ep).is_err() { return -105; }

            let handle = state.sockets.add(sock);

            // Build and enqueue the DNS query packet.
            let mut pkt = [0u8; 512];
            let qlen = build_query(hostname, &mut pkt);
            let dst  = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address(dns_ip)), DNS_PORT);
            let sock2 = state.sockets.get_mut::<UdpSocket>(handle);
            if sock2.send_slice(&pkt[..qlen], dst).is_err() {
                state.sockets.remove(handle);
                return -105;
            }

            let n = hostname.len();
            core::ptr::copy_nonoverlapping(
                hostname.as_ptr(),
                core::ptr::addr_of_mut!((*gdns).hostname) as *mut u8,
                n,
            );
            (*gdns).hostname_len = n;
            (*gdns).handle       = Some(handle);
            (*gdns).active       = true;
            (*gdns).polls        = 0;
        }

        // ── One smoltcp poll ───────────────────────────────────────────────────
        let now = stack::timestamp();
        let mut nic = stack::NicDevice;
        state.iface.poll(now, &mut nic, &mut state.sockets);

        (*gdns).polls += 1;

        let handle = match (*gdns).handle {
            Some(h) => h,
            None    => { (*gdns).active = false; return -105; }
        };

        // Check for DNS reply in socket receive buffer.
        let sock3 = state.sockets.get_mut::<UdpSocket>(handle);
        if let Ok((data, _ep)) = sock3.recv() {
            if let Some(ip) = parse_response(data) {
                state.sockets.remove(handle);
                (*gdns).active = false;
                (*gdns).handle = None;
                return i64::from(u32::from_le_bytes(ip));
            }
        }

        // Time out after MAX_POLLS attempts.
        if (*gdns).polls >= MAX_POLLS {
            if let Some(h) = (*gdns).handle.take() {
                state.sockets.remove(h);
            }
            (*gdns).active = false;
            return -105; // ENONET
        }

        -6 // EAGAIN — caller should sleep briefly and retry
    }
}

// ── DNS packet encoder ─────────────────────────────────────────────────────────

fn build_query(hostname: &[u8], buf: &mut [u8; 512]) -> usize {
    // Fixed query ID — we only ever have one query in flight.
    buf[0] = 0xAB; buf[1] = 0xCD;
    buf[2] = 0x01; buf[3] = 0x00; // flags: recursion desired
    buf[4] = 0x00; buf[5] = 0x01; // QDCOUNT = 1
    buf[6] = 0x00; buf[7] = 0x00; // ANCOUNT = 0
    buf[8] = 0x00; buf[9] = 0x00; // NSCOUNT = 0
    buf[10]= 0x00; buf[11]= 0x00; // ARCOUNT = 0

    let mut pos = 12usize;
    let mut seg_start = 0usize;
    for i in 0..=hostname.len() {
        let end_of_seg = i == hostname.len() || hostname[i] == b'.';
        if end_of_seg {
            let seg = &hostname[seg_start..i];
            if !seg.is_empty() {
                buf[pos] = seg.len() as u8;
                pos += 1;
                buf[pos..pos + seg.len()].copy_from_slice(seg);
                pos += seg.len();
            }
            seg_start = i + 1;
        }
    }
    buf[pos] = 0; pos += 1; // root label
    buf[pos] = 0x00; buf[pos+1] = 0x01; pos += 2; // QTYPE  = A
    buf[pos] = 0x00; buf[pos+1] = 0x01; pos += 2; // QCLASS = IN
    pos
}

// ── DNS response parser ────────────────────────────────────────────────────────

fn parse_response(data: &[u8]) -> Option<[u8; 4]> {
    if data.len() < 12 { return None; }
    let flags = u16::from_be_bytes([data[2], data[3]]);
    if flags & 0x8000 == 0 { return None; } // not a response
    if flags & 0x000F != 0 { return None; } // RCODE != 0

    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;
    if ancount == 0 { return None; }

    let mut pos = 12usize;
    pos = skip_name(data, pos)?;
    pos += 4; // QTYPE + QCLASS

    for _ in 0..ancount {
        pos = skip_name(data, pos)?;
        if pos + 10 > data.len() { return None; }
        let rtype = u16::from_be_bytes([data[pos], data[pos+1]]);
        let rdlen = u16::from_be_bytes([data[pos+8], data[pos+9]]) as usize;
        pos += 10;
        if rtype == 1 && rdlen == 4 && pos + 4 <= data.len() {
            return Some([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
        }
        pos += rdlen;
    }
    None
}

fn skip_name(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        if pos >= data.len() { return None; }
        let len = data[pos] as usize;
        if len == 0 { return Some(pos + 1); }
        if len & 0xC0 == 0xC0 { return Some(pos + 2); }
        pos += 1 + len;
    }
}

// ── Dotted-decimal IP fast path ────────────────────────────────────────────────

fn parse_ipv4(s: &[u8]) -> Option<[u8; 4]> {
    let mut octets = [0u8; 4];
    let mut oi = 0usize;
    let mut cur = 0u32;
    let mut has_digit = false;
    for &b in s {
        if b == b'.' {
            if !has_digit || oi >= 3 { return None; }
            octets[oi] = cur as u8; oi += 1; cur = 0; has_digit = false;
        } else if b.is_ascii_digit() {
            cur = cur * 10 + (b - b'0') as u32;
            if cur > 255 { return None; }
            has_digit = true;
        } else {
            return None;
        }
    }
    if !has_digit || oi != 3 { return None; }
    octets[3] = cur as u8;
    Some(octets)
}
