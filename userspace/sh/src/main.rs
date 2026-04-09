//! OxideOS Shell — minimal userspace shell using fork/exec/waitpid.
//!
//! Built-in commands: echo, cat, ls, pwd, clear, help, exit
//! External commands: any name in the built-in program registry (hello, fib, …)
#![no_std]
#![no_main]

use oxide_rt::{
    exit, fork, waitpid, exec, getchar, sleep_ms,
    print_str, print_bytes, readdir, open, read, close,
};

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

        // Parse: first word is command, rest is args
        let line = match core::str::from_utf8(&line_buf[..len]) {
            Ok(s)  => s.trim(),
            Err(_) => continue,
        };
        if line.is_empty() { continue; }

        let (cmd, args) = match line.find(' ') {
            Some(i) => (line[..i].trim(), line[i + 1..].trim()),
            None    => (line, ""),
        };

        match cmd {
            "exit" | "quit" => exit(0),

            "echo" => {
                print_str(args);
                print_str("\n");
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
            _ if cmd.len() == 0 => {}

            // External program: fork → exec → waitpid
            prog => {
                let pid = fork();
                if pid == 0 {
                    // child: try to exec the program
                    exec(prog);
                    // If exec returned, program not found
                    print_str("sh: ");
                    print_str(prog);
                    print_str(": not found\n");
                    exit(127);
                } else if pid > 0 {
                    waitpid(pid as u32);
                } else {
                    print_str("sh: fork failed\n");
                }
            }
        }

        // Clear the line buffer for next use
        line_buf[..len].fill(0);
    }
}
