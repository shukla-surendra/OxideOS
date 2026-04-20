//! wget — minimal HTTP/1.0 GET client for OxideOS.
//!
//! Usage (with argv):  wget http://example.com/path
//!                     wget 93.184.216.34 80 /
//! Interactive fallback when no args are supplied.
//!
//! DNS resolution is performed by the kernel's built-in resolver.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use oxide_rt::{
    exit, getchar, sleep_ms, write, argc, arg,
    dns_resolve,
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

/// Parse "http://host[:port]/path" or "host port path" style args.
/// Returns (host_bytes, port, path_str).
fn parse_url(url: &str) -> Option<(String, u16, String)> {
    let url = url.strip_prefix("http://").unwrap_or(url);
    // Split off path
    let (host_port, path) = match url.find('/') {
        Some(i) => (&url[..i], &url[i..]),
        None    => (url, "/"),
    };
    // Split host:port
    let (host, port) = match host_port.rfind(':') {
        Some(i) => (&host_port[..i], host_port[i+1..].parse::<u16>().unwrap_or(80)),
        None    => (host_port, 80u16),
    };
    Some((String::from(host), port, String::from(path)))
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    // ── Try to get host/port/path from argv ──────────────────────────────────
    let (host_str, port, path) = if argc() >= 2 {
        let url_arg = arg(1).unwrap_or("");
        match parse_url(url_arg) {
            Some(t) => t,
            None => {
                write(1, b"wget: bad URL\n"); exit(1);
            }
        }
    } else {
        // Interactive fallback
        write(1, b"wget: host (e.g. example.com or 93.184.216.34): ");
        let h = readline();
        write(1, b"wget: port (e.g. 80): ");
        let p_str = readline();
        let p = p_str.trim().parse::<u16>().unwrap_or(80);
        write(1, b"wget: path (e.g. /): ");
        let path_r = readline();
        let path = if path_r.trim().is_empty() { String::from("/") } else { path_r };
        (h, p, path)
    };

    // ── Resolve hostname ─────────────────────────────────────────────────────
    write(1, b"wget: resolving ");
    write(1, host_str.as_bytes());
    write(1, b"...\n");

    let ip = match dns_resolve(host_str.as_bytes()) {
        Some(ip) => ip,
        None => {
            write(1, b"wget: DNS resolution failed\n"); exit(1);
        }
    };

    // ── Print resolved address ───────────────────────────────────────────────
    {
        let mut msg = String::from("wget: connecting to ");
        for (i, &b) in ip.iter().enumerate() {
            if i > 0 { msg.push('.'); }
            // simple u8 → string
            let mut v = b;
            if v == 0 { msg.push('0'); }
            else {
                let mut tmp = [0u8; 3];
                let mut n = 0;
                while v > 0 { tmp[n] = b'0' + v % 10; v /= 10; n += 1; }
                for k in (0..n).rev() { msg.push(tmp[k] as char); }
            }
        }
        msg.push(':');
        msg.push_str(alloc::format!("{}", port).as_str());
        msg.push('\n');
        write(1, msg.as_bytes());
    }

    // ── Open TCP socket ──────────────────────────────────────────────────────
    let sfd = socket(AF_INET, SOCK_STREAM, 0);
    if sfd < 0 { write(1, b"wget: socket() failed\n"); exit(1); }

    let addr = SockAddrIn::new(ip, port);
    let r = connect(sfd, &addr);
    if r < 0 {
        write(1, b"wget: connect() failed\n");
        close_socket(sfd); exit(1);
    }

    sleep_ms(500);

    // ── Send HTTP/1.0 GET ────────────────────────────────────────────────────
    let req = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host_str
    );
    let mut sent = false;
    for _ in 0..20 {
        let n = send(sfd, req.as_bytes());
        if n > 0 { sent = true; break; }
        sleep_ms(100);
    }
    if !sent {
        write(1, b"wget: connection timed out\n");
        close_socket(sfd); exit(1);
    }

    write(1, b"\n--- response ---\n");

    // ── Read response ─────────────────────────────────────────────────────────
    let mut buf = [0u8; 512];
    let mut empty_polls = 0u32;
    loop {
        let n = recv(sfd, &mut buf);
        match n {
            0    => break,
            -11  => { empty_polls += 1; if empty_polls > 300 { break; } sleep_ms(10); }
            n if n > 0 => { empty_polls = 0; write(1, &buf[..n as usize]); }
            _    => break,
        }
    }

    write(1, b"\n--- done ---\n");
    close_socket(sfd);
    exit(0);
}
