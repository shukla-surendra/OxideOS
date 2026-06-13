//! Tiny single-purpose static file server (no external dependencies).

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;

pub fn serve(root: &Path, port: u16) {
    let root = root
        .canonicalize()
        .unwrap_or_else(|_| panic!("{} does not exist — run `make docs-build` first", root.display()));

    let listener = TcpListener::bind(("0.0.0.0", port)).expect("failed to bind port");
    println!("Serving OxideOS docs at http://localhost:{port}/  (Ctrl+C to stop)");

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let root = root.clone();
        thread::spawn(move || {
            let _ = handle(stream, &root);
        });
    }
}

fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    // Drain headers until the blank line terminating the request.
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let path = path.split('?').next().unwrap_or("/");
    let decoded = percent_decode(path);
    let mut rel = decoded.trim_start_matches('/').to_string();
    if rel.is_empty() || rel.ends_with('/') {
        rel.push_str("index.html");
    }

    let requested = root.join(&rel);
    let resolved = requested.canonicalize();

    match resolved {
        Ok(p) if p.starts_with(&root) && p.is_file() => {
            let body = fs::read(&p)?;
            write_response(&mut stream, 200, "OK", mime_type(&p), &body)
        }
        _ => write_response(&mut stream, 404, "Not Found", "text/plain; charset=utf-8", b"404 Not Found"),
    }
}

fn write_response(stream: &mut TcpStream, code: u16, reason: &str, mime: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        code = code,
        reason = reason,
        mime = mime,
        len = body.len(),
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" | "index" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" | "rs" | "toml" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
