//! wget — minimal HTTP/1.0 GET client for OxideOS.
//!
//! Usage (interactive prompt, argv not yet wired):
//!   Host IP  : enter dotted-decimal, e.g. 93.184.216.34
//!   Port     : e.g. 80
//!   Path     : e.g. /
//!
//! Connects via TCP using the kernel's socket syscalls, sends a minimal
//! HTTP/1.0 GET request, and prints the response to stdout.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use oxide_rt::{
    exit, getchar, sleep_ms, write,
    socket, connect, send, recv, close_socket,
    AF_INET, SOCK_STREAM, SockAddrIn,
};

fn getchar_block() -> u8 {
    loop { if let Some(c) = getchar() { return c; } sleep_ms(5); }
}

fn readline() -> String {
    let mut s = String::new();
    loop {
        let c = getchar_block();
        if c == b'\n' || c == b'\r' { break; }
        s.push(c as char);
    }
    s
}

fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let mut parts = s.splitn(4, '.');
    let a = parts.next()?.parse::<u8>().ok()?;
    let b = parts.next()?.parse::<u8>().ok()?;
    let c = parts.next()?.parse::<u8>().ok()?;
    let d = parts.next()?.parse::<u8>().ok()?;
    Some([a, b, c, d])
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    write(1, b"wget: host IP (e.g. 93.184.216.34): ");
    let host_str = readline();
    let ip = match parse_ip(&host_str) {
        Some(ip) => ip,
        None => { write(1, b"bad IP\n"); exit(1); }
    };

    write(1, b"wget: port (e.g. 80): ");
    let port_str = readline();
    let port = port_str.trim().parse::<u16>().unwrap_or(80);

    write(1, b"wget: path (e.g. /): ");
    let path = readline();
    let path = if path.is_empty() { String::from("/") } else { path };

    // Open TCP socket.
    let sfd = socket(AF_INET, SOCK_STREAM, 0);
    if sfd < 0 {
        write(1, b"wget: socket() failed\n"); exit(1);
    }

    // Connect.
    let addr = SockAddrIn::new(ip, port);
    let r = connect(sfd, &addr);
    if r < 0 {
        write(1, b"wget: connect() failed\n");
        close_socket(sfd); exit(1);
    }

    // Wait for the TCP handshake to complete (poll the stack).
    // The kernel polls the stack every ~10 ms in the main loop.
    // We sleep a bit then retry send until it succeeds.
    write(1, b"wget: connecting...\n");
    sleep_ms(500);

    // Build HTTP/1.0 GET request.
    let req = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host_str
    );
    let req_bytes = req.as_bytes();

    // Retry send until the connection is established (may take a few polls).
    let mut sent = false;
    for _ in 0..20 {
        let n = send(sfd, req_bytes);
        if n > 0 { sent = true; break; }
        sleep_ms(100);
    }
    if !sent {
        write(1, b"wget: connection timed out\n");
        close_socket(sfd); exit(1);
    }

    write(1, b"\n--- response ---\n");

    // Read response until EOF.
    let mut buf = [0u8; 512];
    let mut empty_polls = 0u32;
    loop {
        let n = recv(sfd, &mut buf);
        match n {
            0 => break,           // EOF — connection closed cleanly
            -11 => {              // EAGAIN — no data yet
                empty_polls += 1;
                if empty_polls > 300 { break; } // 3-second timeout
                sleep_ms(10);
            }
            n if n > 0 => {
                empty_polls = 0;
                write(1, &buf[..n as usize]);
            }
            _ => break,           // Error
        }
    }

    write(1, b"\n--- done ---\n");
    close_socket(sfd);
    exit(0);
}
