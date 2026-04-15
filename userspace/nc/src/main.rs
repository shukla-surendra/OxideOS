//! /bin/nc — minimal netcat for OxideOS
//!
//! Interactive prompts (like /bin/wget) because argv is not yet wired.
//!
//! Menu:
//!   1) TCP listen  <port>
//!   2) TCP connect <ip> <port>
//!   3) UDP send    <ip> <port>
//!   4) UDP listen  <port>

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use oxide_rt::{
    exit, getchar, sleep_ms, write,
    socket, connect, send, recv, close_socket,
    bind, listen, accept, sendto, recvfrom,
    AF_INET, SOCK_STREAM, SOCK_DGRAM, SockAddrIn,
};

// ── Utilities ─────────────────────────────────────────────────────────────────

fn getchar_block() -> u8 {
    loop { if let Some(c) = getchar() { return c; } sleep_ms(5); }
}

fn readline() -> String {
    let mut s = String::new();
    loop {
        let c = getchar_block();
        if c == b'\n' || c == b'\r' { break; }
        if c == 8 || c == 127 { s.pop(); } else { s.push(c as char); }
    }
    s
}

fn parse_ip(s: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    let mut idx = 0usize;
    let mut cur: u32 = 0;
    for ch in s.chars() {
        if ch == '.' {
            if idx < 4 { out[idx] = cur as u8; }
            idx += 1; cur = 0;
        } else if ch.is_ascii_digit() {
            cur = cur * 10 + (ch as u8 - b'0') as u32;
        }
    }
    if idx < 4 { out[idx] = cur as u8; }
    out
}

fn parse_port(s: &str) -> u16 {
    s.trim().parse::<u16>().unwrap_or(0)
}

// ── TCP relay ─────────────────────────────────────────────────────────────────

/// Line-oriented relay: each stdin line is sent, then we print all received data.
fn relay_tcp(sfd: i64) {
    let mut ibuf = [0u8; 512];
    let mut obuf = [0u8; 1024];
    loop {
        // Read one line from stdin.
        let mut n = 0usize;
        loop {
            let c = getchar_block();
            if n < ibuf.len() { ibuf[n] = c; n += 1; }
            if c == b'\n' { break; }
        }
        if n == 0 { break; }

        let r = send(sfd, &ibuf[..n]);
        if r < 0 { write(1, b"nc: send error\n"); break; }

        // Drain received data (poll for up to ~200 ms).
        let mut polls = 0u32;
        loop {
            sleep_ms(10);
            let got = recv(sfd, &mut obuf);
            if got == 0 { write(1, b"\n[connection closed]\n"); return; }
            if got > 0 { write(1, &obuf[..got as usize]); polls = 0; }
            else { polls += 1; if polls >= 20 { break; } }
        }
    }
}

// ── Mode 1: TCP listen ────────────────────────────────────────────────────────

fn tcp_listen() {
    write(1, b"nc listen: port: ");
    let port = parse_port(&readline());

    let sfd = socket(AF_INET, SOCK_STREAM, 0);
    if sfd < 0 { write(1, b"nc: socket failed\n"); exit(1); }

    let addr = SockAddrIn::new([0, 0, 0, 0], port);
    if bind(sfd, &addr) < 0  { write(1, b"nc: bind failed\n");   close_socket(sfd); exit(1); }
    if listen(sfd, 1) < 0    { write(1, b"nc: listen failed\n"); close_socket(sfd); exit(1); }

    write(1, b"nc: waiting for connection...\n");
    let conn = loop {
        let r = accept(sfd);
        if r >= 0 { break r; }
        sleep_ms(20);
    };
    write(1, b"nc: connection accepted\n");
    relay_tcp(conn);
    close_socket(conn);
    close_socket(sfd);
}

// ── Mode 2: TCP connect ───────────────────────────────────────────────────────

fn tcp_connect() {
    write(1, b"nc connect: host IP: ");
    let ip_str = readline();
    let ip = parse_ip(&ip_str);

    write(1, b"nc connect: port: ");
    let port = parse_port(&readline());

    let sfd = socket(AF_INET, SOCK_STREAM, 0);
    if sfd < 0 { write(1, b"nc: socket failed\n"); exit(1); }

    let addr = SockAddrIn::new(ip, port);
    write(1, b"nc: connecting...\n");
    let r = connect(sfd, &addr);
    if r < 0 { write(1, b"nc: connect failed\n"); close_socket(sfd); exit(1); }

    // Wait for TCP handshake — retry send until it works (like wget).
    let mut connected = false;
    for _ in 0..50 {
        sleep_ms(100);
        let ping = send(sfd, b"");
        if ping >= 0 { connected = true; break; }
    }
    if !connected { write(1, b"nc: timed out\n"); close_socket(sfd); exit(1); }

    write(1, b"nc: connected\n");
    relay_tcp(sfd);
    close_socket(sfd);
}

// ── Mode 3: UDP send ──────────────────────────────────────────────────────────

fn udp_send() {
    write(1, b"nc udp send: host IP: ");
    let ip_str = readline();
    let ip = parse_ip(&ip_str);

    write(1, b"nc udp send: port: ");
    let port = parse_port(&readline());

    let sfd = socket(AF_INET, SOCK_DGRAM, 0);
    if sfd < 0 { write(1, b"nc: socket failed\n"); exit(1); }

    let dst = SockAddrIn::new(ip, port);
    write(1, b"nc: sending UDP (enter lines, empty to quit)\n");

    let mut line = [0u8; 512];
    loop {
        let mut n = 0usize;
        loop {
            let c = getchar_block();
            if n < line.len() { line[n] = c; n += 1; }
            if c == b'\n' { break; }
        }
        if n <= 1 { break; } // empty line
        sendto(sfd, &line[..n], &dst);
    }
    close_socket(sfd);
}

// ── Mode 4: UDP listen ────────────────────────────────────────────────────────

fn udp_listen() {
    write(1, b"nc udp listen: port: ");
    let port = parse_port(&readline());

    let sfd = socket(AF_INET, SOCK_DGRAM, 0);
    if sfd < 0 { write(1, b"nc: socket failed\n"); exit(1); }

    let addr = SockAddrIn::new([0, 0, 0, 0], port);
    if bind(sfd, &addr) < 0 { write(1, b"nc: bind failed\n"); close_socket(sfd); exit(1); }

    write(1, b"nc: listening for UDP datagrams...\n");
    let mut buf = [0u8; 1500];
    loop {
        sleep_ms(20);
        let n = recvfrom(sfd, &mut buf, None);
        if n > 0 { write(1, &buf[..n as usize]); }
    }
}

// ── Entry ─────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    write(1, b"nc: mode?\n");
    write(1, b"  1) TCP listen\n");
    write(1, b"  2) TCP connect\n");
    write(1, b"  3) UDP send\n");
    write(1, b"  4) UDP listen\n");
    write(1, b"choice: ");

    let choice = getchar_block();
    write(1, b"\n");

    match choice {
        b'1' => tcp_listen(),
        b'2' => tcp_connect(),
        b'3' => udp_send(),
        b'4' => udp_listen(),
        _    => { write(1, b"nc: unknown mode\n"); exit(1); }
    }

    exit(0);
}
