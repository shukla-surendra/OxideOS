//! OxideOS Shell — minimal userspace shell using fork/exec/waitpid.
//!
//! Built-in commands: echo, cat, ls, pwd, clear, help, exit
//! External commands: any name in the built-in program registry (hello, fib, …)
//! Redirects: cmd > file, cmd >> file, cmd < file
#![no_std]
#![no_main]

use oxide_rt::{
    exit, fork, waitpid, exec, getchar, sleep_ms,
    print_str, print_bytes, readdir, open, write, read, close, dup2,
};

// ── Open-flag constants (match kernel O_ values) ─────────────────────────────

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_CREAT:  u32 = 0x40;
const O_TRUNC:  u32 = 0x200;
const O_APPEND: u32 = 0x400;

// ── Blocking getchar ─────────────────────────────────────────────────────────

fn getchar_blocking() -> u8 {
    loop {
        if let Some(c) = getchar() { return c; }
        sleep_ms(10);
    }
}

// ── Read one line from stdin, echoing each character ─────────────────────────

fn readline(buf: &mut [u8]) -> usize {
    let mut len = 0usize;
    loop {
        let c = getchar_blocking();
        match c {
            b'\n' | b'\r' => {
                print_str("\n");
                break;
            }
            // Backspace / DEL — erase last character
            8 | 127 => {
                if len > 0 {
                    len -= 1;
                    print_str("\x08 \x08");
                }
            }
            c if c >= 32 && len < buf.len() - 1 => {
                buf[len] = c;
                len += 1;
                print_bytes(core::slice::from_ref(&c));
            }
            _ => {}
        }
    }
    len
}

// ── Redirect parser ────────────────────────────────────────────────────────────
//
// Returns (stripped_line, redirect_fd, redirect_path, append) where
// redirect_fd = -1 means no redirect.

struct Redirect<'a> {
    line:   &'a [u8],  // line with the redirect token removed
    fd:     i32,       // -1 = none
    path:   &'a [u8],
    append: bool,
}

fn parse_redirect(line: &[u8]) -> (&[u8], Option<(bool, &[u8])>) {
    // Scan for '>>' first (must check before '>').
    if let Some(pos) = find_bytes(line, b">>") {
        let before = trim_bytes(&line[..pos]);
        let after  = trim_bytes(&line[pos + 2..]);
        return (before, Some((true, after)));
    }
    if let Some(pos) = find_byte(line, b'>') {
        let before = trim_bytes(&line[..pos]);
        let after  = trim_bytes(&line[pos + 1..]);
        return (before, Some((false, after)));
    }
    (line, None)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > haystack.len() { return None; }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle { return Some(i); }
    }
    None
}
fn find_byte(haystack: &[u8], b: u8) -> Option<usize> {
    haystack.iter().position(|&x| x == b)
}
fn trim_bytes(s: &[u8]) -> &[u8] {
    let s = if let Some(i) = s.iter().position(|&b| b != b' ' && b != b'\t') { &s[i..] } else { s };
    let end = s.iter().rposition(|&b| b != b' ' && b != b'\t').map(|i| i + 1).unwrap_or(0);
    &s[..end]
}

// ── Built-in: cat <path> ─────────────────────────────────────────────────────

fn do_cat(path: &str) {
    if path.is_empty() {
        print_str("cat: missing filename\n");
        return;
    }
    let fd = open(path, 1);
    if fd < 0 {
        print_str("cat: ");
        print_str(path);
        print_str(": not found\n");
        return;
    }
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        print_bytes(&buf[..n as usize]);
    }
    // ensure trailing newline
    print_str("\n");
    close(fd);
}

// ── Built-in: ls [path] ──────────────────────────────────────────────────────

fn do_ls(path: &str) {
    let p = if path.is_empty() { "/" } else { path };
    let mut buf = [0u8; 1024];
    let n = readdir(p, &mut buf);
    if n < 0 {
        print_str("ls: ");
        print_str(p);
        print_str(": not found\n");
        return;
    }
    if n == 0 {
        print_str("(empty)\n");
        return;
    }
    print_bytes(&buf[..n as usize]);
}

// ── Built-in: help ───────────────────────────────────────────────────────────

fn print_help() {
    print_str("Built-in commands:\n");
    print_str("  echo <text>     print text\n");
    print_str("  cat  <file>     print file contents\n");
    print_str("  ls   [dir]      list directory (default /)\n");
    print_str("  pwd             print working directory\n");
    print_str("  clear           scroll screen\n");
    print_str("  help            this message\n");
    print_str("  exit            quit the shell\n");
    print_str("\nPrograms (run by name):\n");
    print_str("  hello  counter  sysinfo  input  fib  primes\n");
    print_str("  countdown  spinner  filetest  hello_rust\n");
}

// ── Main entry ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    print_str("\nOxideOS Shell v0.1\n");
    print_str("Type 'help' for available commands.\n\n");

    let mut line_buf  = [0u8; 256];
    let mut cwd: &str = "/";

    loop {
        // Prompt
        print_str(cwd);
        print_str(" $ ");

        let len = readline(&mut line_buf);
        if len == 0 { continue; }

        // Parse redirects first, then split cmd/args.
        let raw_bytes = &line_buf[..len];
        let (cmd_bytes, redir) = parse_redirect(raw_bytes);

        let line = match core::str::from_utf8(cmd_bytes) {
            Ok(s)  => s.trim(),
            Err(_) => continue,
        };
        if line.is_empty() { continue; }

        let (cmd, args) = match line.find(' ') {
            Some(i) => (line[..i].trim(), line[i + 1..].trim()),
            None    => (line, ""),
        };

        // If there's a redirect, open the file and set up stdout before the command.
        // Built-ins that produce output must also honour this.
        let redir_fd: i32 = if let Some((append, path_bytes)) = redir {
            let path_str = match core::str::from_utf8(path_bytes) {
                Ok(s) => s.trim(),
                Err(_) => { print_str("sh: bad redirect path\n"); continue; }
            };
            let flags = O_WRONLY | O_CREAT | if append { O_APPEND } else { O_TRUNC };
            let fd = open(path_str, flags);
            if fd < 0 {
                print_str("sh: cannot open '");
                print_str(path_str);
                print_str("' for writing\n");
                continue;
            }
            fd as i32
        } else {
            -1
        };

        match cmd {
            "exit" | "quit" => exit(0),

            "echo" => {
                if redir_fd >= 0 {
                    let b = args.as_bytes();
                    let _ = write(redir_fd, b);
                    let _ = write(redir_fd, b"\n");
                    close(redir_fd);
                } else {
                    print_str(args);
                    print_str("\n");
                }
            }

            "cat" => do_cat(args),

            "ls" => do_ls(args),

            "pwd" => {
                print_str(cwd);
                print_str("\n");
            }

            "clear" => {
                for _ in 0..50usize { print_str("\n"); }
            }

            "help" => print_help(),

            // Ignore blank / unknown single-char input
            _ if cmd.len() == 0 => {
                if redir_fd >= 0 { close(redir_fd); }
            }

            // External program: fork → exec → waitpid
            prog => {
                let pid = fork();
                if pid == 0 {
                    // child: redirect stdout if requested
                    if redir_fd >= 0 {
                        dup2(redir_fd, 1);
                        close(redir_fd);
                    }
                    exec(prog);
                    print_str("sh: ");
                    print_str(prog);
                    print_str(": not found\n");
                    exit(127);
                } else if pid > 0 {
                    if redir_fd >= 0 { close(redir_fd); }
                    waitpid(pid as u32);
                } else {
                    if redir_fd >= 0 { close(redir_fd); }
                    print_str("sh: fork failed\n");
                }
            }
        }

        // Clear the line buffer for next use
        line_buf[..len].fill(0);
    }
}
