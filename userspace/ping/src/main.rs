//! ping — TCP-based connectivity checker for OxideOS.
//!
//! Since ICMP is not exposed to userspace, this tool probes reachability by:
//!   1. Resolving the hostname via the kernel DNS client.
//!   2. Opening a TCP connection to the target host:port.
//!   3. Measuring round-trip time with the 100 Hz system timer (10 ms / tick).
//!
//! Usage (argv):  ping <host> [port]
//! Interactive:   ping                (prompts for host and port)
//!
//! Port defaults to 80.  Use port 53 to probe DNS servers directly.
#![no_std]
#![no_main]

extern crate alloc;
use alloc::string::String;

use oxide_rt::{
    exit, getchar, sleep_ms, write, argc, arg, get_time,
    dns_resolve,
    socket, connect, send, close_socket,
    AF_INET, SOCK_STREAM, SockAddrIn,
};

// The kernel timer runs at 100 Hz, so 1 tick = 10 ms.
fn ticks_to_ms(ticks: u64) -> u64 { ticks * 1000 / 100 }

// ── I/O helpers ────────────────────────────────────────────────────────────────

fn getchar_block() -> u8 {
    loop { if let Some(c) = getchar() { return c; } sleep_ms(5); }
}

fn readline() -> String {
    let mut s = String::new();
    loop {
        let c = getchar_block();
        if c == b'\n' || c == b'\r' { break; }
        if (c == 8 || c == 127) && !s.is_empty() { s.pop(); }
        else if c >= 0x20 { s.push(c as char); }
    }
    s
}

// ── Number formatting (no_std, no format!) ────────────────────────────────────

fn write_u64(n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut v = n;
    if v == 0 { i -= 1; buf[i] = b'0'; }
    while v > 0 { i -= 1; buf[i] = b'0' + (v % 10) as u8; v /= 10; }
    write(1, &buf[i..]);
}

fn write_ip(ip: [u8; 4]) {
    for (i, &b) in ip.iter().enumerate() {
        if i > 0 { write(1, b"."); }
        write_u64(b as u64);
    }
}

fn parse_port(s: &str) -> u16 {
    let mut n: u32 = 0;
    for ch in s.trim().chars() {
        if ch.is_ascii_digit() { n = n * 10 + (ch as u32 - '0' as u32); }
    }
    n.min(65535) as u16
}

// ── Single probe ───────────────────────────────────────────────────────────────

/// Try one TCP probe to `ip:port`.  Returns `Some(rtt_ms)` on success.
fn tcp_probe(ip: [u8; 4], port: u16) -> Option<u64> {
    let sfd = socket(AF_INET, SOCK_STREAM, 0);
    if sfd < 0 { return None; }

    let addr = SockAddrIn::new(ip, port);
    let t0   = get_time();

    let r = connect(sfd, &addr);
    if r < 0 { close_socket(sfd); return None; }

    // Wait up to 5 s for the TCP handshake to complete (smoltcp is async).
    let mut connected = false;
    for _ in 0..50 {
        sleep_ms(100);
        // An empty send returns ≥ 0 once the socket is established.
        if send(sfd, b"") >= 0 { connected = true; break; }
    }

    let t1 = get_time();
    close_socket(sfd);

    if connected { Some(ticks_to_ms(t1 - t0)) } else { None }
}

// ── Main ───────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    // ── Parse arguments or prompt ─────────────────────────────────────────
    let (host_str, port) = if argc() >= 2 {
        let h = String::from(arg(1).unwrap_or(""));
        let p = if argc() >= 3 {
            parse_port(arg(2).unwrap_or("80"))
        } else {
            80
        };
        (h, p)
    } else {
        write(1, b"ping: host (IP or hostname): ");
        let h = readline();
        write(1, b"ping: port [80]: ");
        let p_str = readline();
        let p = if p_str.trim().is_empty() { 80 } else { parse_port(&p_str) };
        (h, p)
    };

    let host_bytes = host_str.as_bytes();
    if host_bytes.is_empty() {
        write(1, b"ping: no host specified\n");
        exit(1);
    }

    write(1, b"\n");
    write(1, b"PING ");
    write(1, host_bytes);
    write(1, b" port ");
    write_u64(port as u64);
    write(1, b" (TCP)\n");
    write(1, b"----------------------------------------\n");

    // ── Step 1: DNS resolution ────────────────────────────────────────────
    write(1, b"Resolving ");
    write(1, host_bytes);
    write(1, b" ... ");

    let t_dns0 = get_time();
    let ip = match dns_resolve(host_bytes) {
        Some(ip) => ip,
        None => {
            write(1, b"FAILED\n\n");
            write(1, b"Status: UNREACHABLE (DNS failed)\n");
            exit(1);
        }
    };
    let dns_ms = ticks_to_ms(get_time() - t_dns0);

    write_ip(ip);
    write(1, b"  (");
    write_u64(dns_ms);
    write(1, b" ms)\n");

    // ── Step 2: TCP probes ────────────────────────────────────────────────
    const PROBES: usize = 4;
    let mut success = 0usize;
    let mut total_ms = 0u64;
    let mut min_ms = u64::MAX;
    let mut max_ms = 0u64;

    for seq in 1..=PROBES {
        write(1, b"Probe ");
        write_u64(seq as u64);
        write(1, b"/");
        write_u64(PROBES as u64);
        write(1, b": connecting to ");
        write_ip(ip);
        write(1, b":");
        write_u64(port as u64);
        write(1, b" ... ");

        match tcp_probe(ip, port) {
            Some(rtt) => {
                write(1, b"OK  rtt=");
                write_u64(rtt);
                write(1, b" ms\n");
                success += 1;
                total_ms += rtt;
                if rtt < min_ms { min_ms = rtt; }
                if rtt > max_ms { max_ms = rtt; }
            }
            None => {
                write(1, b"TIMEOUT\n");
            }
        }

        if seq < PROBES { sleep_ms(500); }
    }

    // ── Step 3: Summary ───────────────────────────────────────────────────
    write(1, b"\n--- ping statistics ---\n");
    write(1, b"Host:      ");
    write(1, host_bytes);
    write(1, b" (");
    write_ip(ip);
    write(1, b")\n");

    write(1, b"DNS:       ");
    write_u64(dns_ms);
    write(1, b" ms\n");

    write(1, b"Probes:    ");
    write_u64(success as u64);
    write(1, b"/");
    write_u64(PROBES as u64);
    write(1, b" succeeded\n");

    if success > 0 {
        write(1, b"RTT min:   ");
        write_u64(min_ms);
        write(1, b" ms\n");
        write(1, b"RTT max:   ");
        write_u64(max_ms);
        write(1, b" ms\n");
        write(1, b"RTT avg:   ");
        write_u64(total_ms / success as u64);
        write(1, b" ms\n");
    }

    write(1, b"\nStatus: ");
    if success == PROBES {
        write(1, b"REACHABLE\n");
    } else if success > 0 {
        write(1, b"DEGRADED (partial loss)\n");
    } else {
        write(1, b"UNREACHABLE\n");
    }

    exit(0);
}
