//! Minimal DNS A-record resolver.
//!
//! Sends a single UDP DNS query to the DNS server stored in NET_CONFIG.dns
//! (defaults to 10.0.2.3 — the QEMU slirp resolver), waits up to ~2 seconds
//! for a response, then returns the first IPv4 address in the answer section.
//!
//! Usage:
//!   let ip = dns::resolve(b"example.com");

extern crate alloc;

use smoltcp::socket::udp::{self, Socket as UdpSocket};
use smoltcp::wire::{IpEndpoint, IpAddress, Ipv4Address};

use super::stack::{self, NET};

// ── Public API ──────────────────────────────────────────────────────────────

/// Resolve a hostname to an IPv4 address.
/// Returns `None` on timeout or parse error.
/// If `hostname` looks like a dotted-decimal IP, it is parsed directly.
pub fn resolve(hostname: &[u8]) -> Option<[u8; 4]> {
    // Fast path: already a dotted-decimal IP.
    if let Some(ip) = parse_ipv4(hostname) {
        return Some(ip);
    }

    let dns_ip = stack::get_dns();

    unsafe { do_query(hostname, dns_ip) }
}

// ── Internal ────────────────────────────────────────────────────────────────

const DNS_PORT:  u16 = 53;
const LOCAL_PORT: u16 = 5353;
const QUERY_ID:  u16 = 0xABCD;

unsafe fn do_query(hostname: &[u8], dns_ip: [u8; 4]) -> Option<[u8; 4]> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(NET);
        let state = match &mut *ptr { Some(s) => s, None => return None };

        // Allocate a UDP socket.
        let rx = udp::PacketBuffer::new(
            alloc::vec![udp::PacketMetadata::EMPTY; 4],
            alloc::vec![0u8; 512],
        );
        let tx = udp::PacketBuffer::new(
            alloc::vec![udp::PacketMetadata::EMPTY; 4],
            alloc::vec![0u8; 512],
        );
        let mut sock = UdpSocket::new(rx, tx);
        let local_ep = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::UNSPECIFIED), LOCAL_PORT);
        if sock.bind(local_ep).is_err() { return None; }
        let handle = state.sockets.add(sock);

        // Build DNS query.
        let mut pkt = [0u8; 512];
        let qlen = build_query(hostname, &mut pkt);

        // Send query.
        let dst = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address(dns_ip)), DNS_PORT);
        let sock2 = state.sockets.get_mut::<UdpSocket>(handle);
        if sock2.send_slice(&pkt[..qlen], dst).is_err() {
            state.sockets.remove(handle);
            return None;
        }

        // Poll until we receive a response (up to ~2 seconds).
        let mut result = None;
        for _ in 0..200u32 {
            let now = stack::timestamp();
            let mut nic = stack::NicDevice;
            state.iface.poll(now, &mut nic, &mut state.sockets);

            let sock3 = state.sockets.get_mut::<UdpSocket>(handle);
            if let Ok((data, _ep)) = sock3.recv() {
                result = parse_response(data);
                break;
            }

            // ~10 ms busy wait.
            for _ in 0..500_000u32 { core::hint::spin_loop(); }
        }

        state.sockets.remove(handle);
        result
    }
}

// ── DNS packet encoder ──────────────────────────────────────────────────────

fn build_query(hostname: &[u8], buf: &mut [u8; 512]) -> usize {
    // Header (12 bytes)
    buf[0] = (QUERY_ID >> 8) as u8;
    buf[1] = (QUERY_ID & 0xFF) as u8;
    buf[2] = 0x01; // flags: standard query, recursion desired
    buf[3] = 0x00;
    buf[4] = 0x00; buf[5] = 0x01; // QDCOUNT = 1
    buf[6] = 0x00; buf[7] = 0x00; // ANCOUNT = 0
    buf[8] = 0x00; buf[9] = 0x00; // NSCOUNT = 0
    buf[10] = 0x00; buf[11] = 0x00; // ARCOUNT = 0

    // QNAME: split hostname on '.', each segment = length byte + bytes.
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

    // QTYPE = A (1), QCLASS = IN (1)
    buf[pos] = 0x00; buf[pos + 1] = 0x01; pos += 2;
    buf[pos] = 0x00; buf[pos + 1] = 0x01; pos += 2;

    pos
}

// ── DNS response parser ─────────────────────────────────────────────────────

fn parse_response(data: &[u8]) -> Option<[u8; 4]> {
    if data.len() < 12 { return None; }

    // Verify it's a response (bit 15 of flags = 1) and no error (RCODE == 0).
    let flags = u16::from_be_bytes([data[2], data[3]]);
    if flags & 0x8000 == 0 { return None; } // not a response
    if flags & 0x000F != 0 { return None; } // RCODE != 0 (error)

    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;
    if ancount == 0 { return None; }

    // Skip the question section.
    let mut pos = 12usize;
    pos = skip_name(data, pos)?;
    pos += 4; // QTYPE + QCLASS

    // Parse answer records.
    for _ in 0..ancount {
        pos = skip_name(data, pos)?;
        if pos + 10 > data.len() { return None; }
        let rtype  = u16::from_be_bytes([data[pos], data[pos + 1]]);
        // let rclass = u16::from_be_bytes([data[pos+2], data[pos+3]]);
        // let ttl    = u32::from_be_bytes([data[pos+4], ..]);
        let rdlen  = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;

        if rtype == 1 && rdlen == 4 && pos + 4 <= data.len() {
            return Some([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        }
        pos += rdlen;
    }
    None
}

/// Skip a DNS name (handles message compression pointers).
fn skip_name(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        if pos >= data.len() { return None; }
        let len = data[pos] as usize;
        if len == 0 { return Some(pos + 1); }
        if len & 0xC0 == 0xC0 {
            // Compression pointer — 2 bytes total, name ends here.
            return Some(pos + 2);
        }
        pos += 1 + len;
    }
}

// ── Dotted-decimal IP parser ────────────────────────────────────────────────

fn parse_ipv4(s: &[u8]) -> Option<[u8; 4]> {
    let mut octets = [0u8; 4];
    let mut oi = 0usize;
    let mut cur = 0u32;
    let mut has_digit = false;
    for &b in s {
        if b == b'.' {
            if !has_digit || oi >= 3 { return None; }
            octets[oi] = cur as u8;
            oi += 1;
            cur = 0;
            has_digit = false;
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
