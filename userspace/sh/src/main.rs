//! OxideOS Shell — minimal userspace shell using fork/exec/waitpid.
//!
//! Features:
//!   - Built-in commands: echo, cat, ls, pwd, clear, help, export, exit
//!   - External commands: any name in the built-in program registry
//!   - Output redirects: cmd > file, cmd >> file
//!   - Pipelines: cmd1 | cmd2 | ... | cmdN (up to 8 stages)
//!   - Environment variables: $VAR expansion, export VAR=val
#![no_std]
#![no_main]

use oxide_rt::{
    exit, fork, waitpid, exec_args, getchar, sleep_ms,
    print_str, print_bytes, readdir, open, write, read, close, dup2, pipe,
    setenv, getenv_bytes,
};

// ── Open-flag constants ────────────────────────────────────────────────────────

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

// ── Byte-slice helpers ────────────────────────────────────────────────────────

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

// ── Redirect parser ───────────────────────────────────────────────────────────

fn parse_redirect(line: &[u8]) -> (&[u8], Option<(bool, &[u8])>) {
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

// ── Variable expansion ────────────────────────────────────────────────────────
// Replaces $VAR occurrences with the value from the env store.
// Works on raw byte slices; output written to `out`, returns bytes written.

fn expand_vars(input: &[u8], out: &mut [u8; 512]) -> usize {
    let mut i = 0usize;
    let mut o = 0usize;
    while i < input.len() && o < out.len() - 1 {
        if input[i] == b'$' {
            // Find end of variable name (alphanumeric + _).
            let start = i + 1;
            let mut end = start;
            while end < input.len() && (input[end].is_ascii_alphanumeric() || input[end] == b'_') {
                end += 1;
            }
            if end > start {
                let varname = &input[start..end];
                let mut vbuf = [0u8; 256];
                let key_str = match core::str::from_utf8(varname) { Ok(s) => s, Err(_) => { i += 1; continue; } };
                let n = getenv_bytes(key_str, &mut vbuf);
                if n > 0 {
                    let n = n as usize;
                    let copy = n.min(out.len() - 1 - o);
                    out[o..o + copy].copy_from_slice(&vbuf[..copy]);
                    o += copy;
                }
                i = end;
                continue;
            }
        }
        out[o] = input[i];
        o += 1;
        i += 1;
    }
    o
}

// ── Pipeline split ────────────────────────────────────────────────────────────
// Splits `line` on '|' into up to 8 trimmed segments.
// Returns the number of segments found.

fn split_pipes<'a>(line: &'a [u8], segs: &mut [&'a [u8]; 8]) -> usize {
    let mut n = 0usize;
    let mut rest = line;
    loop {
        if n >= 8 { break; }
        match find_byte(rest, b'|') {
            Some(pos) => {
                segs[n] = trim_bytes(&rest[..pos]);
                n += 1;
                rest = &rest[pos + 1..];
            }
            None => {
                segs[n] = trim_bytes(rest);
                n += 1;
                break;
            }
        }
    }
    n
}

// ── Pipeline executor ─────────────────────────────────────────────────────────
// Runs a slice of command segments as a pipeline.
// If `final_out_fd >= 0` it replaces stdout of the last stage.

fn run_pipeline(segs: &[&[u8]], final_out_fd: i32) {
    let n = segs.len();
    if n == 0 { return; }

    // Allocate n-1 pipes.
    let mut pipe_rd = [-1i32; 7];
    let mut pipe_wr = [-1i32; 7];
    for i in 0..(n - 1) {
        let mut r = -1i32;
        let mut w = -1i32;
        pipe(&mut r, &mut w);
        pipe_rd[i] = r;
        pipe_wr[i] = w;
    }

    let mut child_pids = [0u32; 8];

    for i in 0..n {
        let pid = fork();
        if pid == 0 {
            // ── Child: wire stdin/stdout ──────────────────────────────────────
            if i > 0        { dup2(pipe_rd[i - 1], 0); }
            if i < n - 1    { dup2(pipe_wr[i],     1); }
            else if final_out_fd >= 0 { dup2(final_out_fd, 1); }

            // Close all pipe ends.
            for j in 0..(n - 1) {
                close(pipe_rd[j]);
                close(pipe_wr[j]);
            }
            if final_out_fd >= 0 { close(final_out_fd); }

            // Parse and exec this segment.
            let seg = segs[i];
            if seg.is_empty() { exit(0); }
            let seg_str = match core::str::from_utf8(seg) { Ok(s) => s.trim(), Err(_) => exit(1) };
            if seg_str.is_empty() { exit(0); }
            let (prog, args) = match seg_str.find(' ') {
                Some(idx) => (seg_str[..idx].trim(), seg_str[idx + 1..].trim()),
                None      => (seg_str, ""),
            };
            exec_args(prog, args);
            print_str("sh: ");
            print_str(prog);
            print_str(": not found\n");
            exit(127);
        } else if pid > 0 {
            child_pids[i] = pid as u32;
        } else {
            print_str("sh: fork failed\n");
        }
    }

    // Parent: close all pipe ends.
    for i in 0..(n - 1) {
        close(pipe_rd[i]);
        close(pipe_wr[i]);
    }
    if final_out_fd >= 0 { close(final_out_fd); }

    // Wait for all children.
    for i in 0..n {
        if child_pids[i] > 0 { waitpid(child_pids[i]); }
    }
}

// ── Built-in: cat <path> ──────────────────────────────────────────────────────

fn do_cat(path: &str) {
    if path.is_empty() { print_str("cat: missing filename\n"); return; }
    let fd = open(path, O_RDONLY);
    if fd < 0 {
        print_str("cat: "); print_str(path); print_str(": not found\n");
        return;
    }
    let mut buf = [0u8; 512];
    loop {
        let n = read(fd, &mut buf);
        if n <= 0 { break; }
        print_bytes(&buf[..n as usize]);
    }
    print_str("\n");
    close(fd);
}

// ── Built-in: ls [path] ───────────────────────────────────────────────────────

fn do_ls(path: &str) {
    let p = if path.is_empty() { "/" } else { path };
    let mut buf = [0u8; 1024];
    let n = readdir(p, &mut buf);
    if n < 0 {
        print_str("ls: "); print_str(p); print_str(": not found\n");
        return;
    }
    if n == 0 { print_str("(empty)\n"); return; }
    print_bytes(&buf[..n as usize]);
}

// ── Built-in: help ────────────────────────────────────────────────────────────

fn print_help() {
    print_str("Built-in commands:\n");
    print_str("  echo <text>          print text\n");
    print_str("  cat  <file>          print file contents\n");
    print_str("  ls   [dir]           list directory\n");
    print_str("  pwd                  print working directory\n");
    print_str("  export VAR=val       set environment variable\n");
    print_str("  clear                scroll screen\n");
    print_str("  help                 this message\n");
    print_str("  exit                 quit the shell\n");
    print_str("\nExternal programs:\n");
    print_str("  ls cat grep wc head tail sort echo sleep kill touch\n");
    print_str("  ps cp mkdir rm mv wget edit nc filemanager terminal\n");
    print_str("\nPipelines:   cmd1 | cmd2 | cmd3\n");
    print_str("Redirects:   cmd > file   cmd >> file\n");
    print_str("Variables:   export PATH=/bin    echo $HOME\n");
}

// ── Main entry ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    print_str("\nOxideOS Shell v0.2\n");
    print_str("Type 'help' for available commands.\n\n");

    let mut line_buf   = [0u8; 256];
    let mut expand_buf = [0u8; 512];
    let mut cwd: &str  = "/";

    loop {
        // Prompt.
        print_str(cwd);
        print_str(" $ ");

        let len = readline(&mut line_buf);
        if len == 0 { continue; }

        let raw_bytes = &line_buf[..len];

        // ── Redirect parsing ──────────────────────────────────────────────────
        let (cmd_bytes_raw, redir) = parse_redirect(raw_bytes);

        // ── Variable expansion ────────────────────────────────────────────────
        let exp_len = expand_vars(cmd_bytes_raw, &mut expand_buf);
        let cmd_bytes = &expand_buf[..exp_len];

        // ── Pipeline split ────────────────────────────────────────────────────
        let mut pipe_segs: [&[u8]; 8] = [b""; 8];
        let n_segs = split_pipes(cmd_bytes, &mut pipe_segs);

        // Open the redirect file if any (applies to last pipeline stage).
        let redir_fd: i32 = if let Some((append, path_bytes)) = redir {
            let path_str = match core::str::from_utf8(path_bytes) {
                Ok(s) => s.trim(),
                Err(_) => { print_str("sh: bad redirect path\n"); continue; }
            };
            let flags = O_WRONLY | O_CREAT | if append { O_APPEND } else { O_TRUNC };
            let fd = open(path_str, flags);
            if fd < 0 {
                print_str("sh: cannot open '"); print_str(path_str); print_str("' for writing\n");
                continue;
            }
            fd as i32
        } else {
            -1
        };

        // ── Pipeline: 2 or more stages → run_pipeline ─────────────────────────
        if n_segs > 1 {
            run_pipeline(&pipe_segs[..n_segs], redir_fd);
            line_buf[..len].fill(0);
            expand_buf[..exp_len].fill(0);
            continue;
        }

        // ── Single command ────────────────────────────────────────────────────
        let line = match core::str::from_utf8(pipe_segs[0]) {
            Ok(s)  => s.trim(),
            Err(_) => continue,
        };
        if line.is_empty() { if redir_fd >= 0 { close(redir_fd); } continue; }

        let (cmd, args) = match line.find(' ') {
            Some(i) => (line[..i].trim(), line[i + 1..].trim()),
            None    => (line, ""),
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

            "pwd" => { print_str(cwd); print_str("\n"); }

            "clear" => { for _ in 0..50usize { print_str("\n"); } }

            "help" => print_help(),

            // export VAR=val  or  export VAR (no value → empty string)
            "export" => {
                if let Some(eq) = args.find('=') {
                    let key = args[..eq].trim();
                    let val = args[eq + 1..].trim();
                    setenv(key, val);
                } else if !args.is_empty() {
                    // export VAR with no value: no-op (already set or unset)
                }
            }

            _ if cmd.len() == 0 => {
                if redir_fd >= 0 { close(redir_fd); }
            }

            // External program: fork → exec → waitpid.
            prog => {
                let pid = fork();
                if pid == 0 {
                    if redir_fd >= 0 { dup2(redir_fd, 1); close(redir_fd); }
                    exec_args(prog, args);
                    print_str("sh: "); print_str(prog); print_str(": not found\n");
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

        line_buf[..len].fill(0);
        expand_buf[..exp_len].fill(0);
    }
}
