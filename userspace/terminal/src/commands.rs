//! Command dispatch: tab-completion, built-ins, external fork/exec, pipes, submit.
//!
//! Entry point is `submit` — called when the user presses Enter.

use oxide_rt::{exit, fork, waitpid, exec, getpid, pipe, dup2, close};
use crate::constants::*;
use crate::fixstr::FixStr;
use crate::fmt::{fmt_u32, fmt_i64};
use crate::terminal::Terminal;

// ── Tab completion ────────────────────────────────────────────────────────────

pub fn tab_complete(term: &mut Terminal) {
    // Copy current input into a local buffer to release borrow on `term`.
    let mut ibuf = [0u8; 256];
    let ilen = term.input.len.min(255);
    ibuf[..ilen].copy_from_slice(&term.input.buf[..ilen]);
    let input = core::str::from_utf8(&ibuf[..ilen]).unwrap_or("");

    if input.contains(' ') { return; } // only complete the first word

    const BUILTINS: &[&str] = &["clear","echo","exit","help","pid","programs","ticks"];
    let mut match_buf = [("", ""); 32];
    let mut match_count = 0usize;

    for &b in BUILTINS {
        if b.starts_with(input) && match_count < match_buf.len() {
            match_buf[match_count] = (b, ""); match_count += 1;
        }
    }
    for &(name, desc) in PROGRAMS {
        if name.starts_with(input) && match_count < match_buf.len() {
            match_buf[match_count] = (name, desc); match_count += 1;
        }
    }

    if match_count == 0 { return; }
    if match_count == 1 {
        term.input.clear();
        term.input.push_str(match_buf[0].0);
        term.cursor = term.input.len;
        return;
    }

    // Multiple matches: show them and extend to longest common prefix.
    term.log("", COL_DEFAULT);
    for i in 0..match_count {
        let (name, desc) = match_buf[i];
        let mut line = FixStr::<LINE_CAP>::new();
        line.push_str("  "); line.push_str(name);
        if !desc.is_empty() {
            for _ in name.len()..14 { line.push(b' '); }
            line.push_str(desc);
        }
        term.log(line.as_str(), COL_PROG_NAME);
    }
    let first = match_buf[0].0;
    let mut prefix_len = first.len();
    for i in 1..match_count {
        prefix_len = prefix_len.min(
            first.bytes().zip(match_buf[i].0.bytes()).take_while(|(a, b)| a == b).count()
        );
    }
    if prefix_len > ilen {
        term.input.clear();
        term.input.push_str(&first[..prefix_len]);
        term.cursor = term.input.len;
    }
}

// ── Built-in commands ─────────────────────────────────────────────────────────

fn cmd_help(term: &mut Terminal) {
    term.log("", COL_DEFAULT);
    term.log("  Built-in commands:", COL_INFO);
    term.log("  clear          Clear the terminal", COL_DEFAULT);
    term.log("  echo <text>    Print text to terminal", COL_DEFAULT);
    term.log("  programs       List all available programs", COL_DEFAULT);
    term.log("  pid            Show current process ID", COL_DEFAULT);
    term.log("  ticks          Show kernel tick counter", COL_DEFAULT);
    term.log("  exit           Exit the terminal", COL_DEFAULT);
    term.log("", COL_DEFAULT);
    term.log("  Tips:", COL_INFO);
    term.log("  Tab            Auto-complete command", COL_DIM);
    term.log("  Up / Down      Scroll command history", COL_DIM);
    term.log("  PgUp / PgDn    Scroll output history", COL_DIM);
    term.log("  cmd &          Run in background", COL_DIM);
    term.log("  cmd1 | cmd2    Pipe stdout to stdin", COL_DIM);
}

fn cmd_programs(term: &mut Terminal) {
    term.log("", COL_DEFAULT);
    term.log("  Available programs:", COL_INFO);
    term.log("  ──────────────────────────────────────────────", COL_SEPARATOR);
    for &(name, desc) in PROGRAMS {
        let mut line = FixStr::<LINE_CAP>::new();
        line.push_str("  "); line.push_str(name);
        for _ in name.len()..12 { line.push(b' '); }
        line.push_str("  "); line.push_str(desc);
        term.log(line.as_str(), COL_DEFAULT);
    }
    term.log("  ──────────────────────────────────────────────", COL_SEPARATOR);
    term.log("  Shell: sh  (type 'sh' for an interactive shell)", COL_DIM);
}

/// Returns `true` if `cmd` is a built-in and was handled.
pub fn run_builtin(term: &mut Terminal, cmd: &str, args: &str) -> bool {
    match cmd {
        "clear" => {
            term.history.count  = 0;
            term.history.scroll = 0;
            term.dirty = true;
            true
        }
        "help"              => { cmd_help(term); true }
        "programs" | "ls"   => { cmd_programs(term); true }
        "exit" | "quit"     => exit(0),
        "echo"              => { term.log(args, COL_DEFAULT); true }
        "pid" => {
            let pid = getpid();
            let mut b = [0u8; 16];
            let mut line = FixStr::<32>::new();
            line.push_str("PID: "); line.push_str(fmt_u32(&mut b, pid));
            term.log(line.as_str(), COL_INFO);
            true
        }
        "ticks" => {
            let t = oxide_rt::get_time();
            let mut b = [0u8; 24];
            let mut line = FixStr::<48>::new();
            line.push_str("Ticks: "); line.push_str(fmt_i64(&mut b, t as i64));
            term.log(line.as_str(), COL_INFO);
            true
        }
        _ => false,
    }
}

// ── External command execution ────────────────────────────────────────────────

pub fn run_external(term: &mut Terminal, cmd: &str, background: bool) {
    let child = fork();
    if child < 0 { term.log("[error] fork failed", COL_ERROR); return; }
    if child == 0 { let _ = exec(cmd); exit(127); }
    if background {
        let mut b = [0u8; 16];
        let mut line = FixStr::<64>::new();
        line.push_str("spawned PID "); line.push_str(fmt_u32(&mut b, child as u32));
        term.log(line.as_str(), COL_SUCCESS);
        return;
    }
    let code = waitpid(child as u32);
    if code == 127 {
        let mut line = FixStr::<LINE_CAP>::new();
        line.push_str("[error] '"); line.push_str(cmd);
        line.push_str("' not found  (try 'programs' to list)");
        term.log(line.as_str(), COL_ERROR);
    } else if code != 0 {
        let mut b = [0u8; 24];
        let mut line = FixStr::<64>::new();
        line.push_str("exited "); line.push_str(fmt_i64(&mut b, code));
        term.log(line.as_str(), COL_DIM);
    }
}

// ── Pipe execution ────────────────────────────────────────────────────────────

pub fn run_pipe(term: &mut Terminal, left_cmd: &str, right_cmd: &str) {
    let mut r: i32 = -1; let mut w: i32 = -1;
    if pipe(&mut r, &mut w) < 0 { term.log("[error] pipe() failed", COL_ERROR); return; }

    let left_pid = fork();
    if left_pid < 0 {
        close(r); close(w);
        term.log("[error] fork failed (left)", COL_ERROR); return;
    }
    if left_pid == 0 { close(r); dup2(w, 1); close(w); let _ = exec(left_cmd); exit(127); }

    let right_pid = fork();
    if right_pid < 0 {
        close(r); close(w); term.log("[error] fork failed (right)", COL_ERROR);
        waitpid(left_pid as u32); return;
    }
    if right_pid == 0 { close(w); dup2(r, 0); close(r); let _ = exec(right_cmd); exit(127); }

    close(r); close(w);
    waitpid(left_pid as u32);
    waitpid(right_pid as u32);
}

// ── Command submit ────────────────────────────────────────────────────────────

/// Process the current input line: echo it, save to history, and run it.
pub fn submit(term: &mut Terminal) {
    // Copy into a local buffer to release borrow on term.input.
    let mut local = [0u8; 256];
    let len = term.input.len.min(255);
    local[..len].copy_from_slice(&term.input.buf[..len]);
    let raw     = core::str::from_utf8(&local[..len]).unwrap_or("");
    let trimmed = raw.trim();

    term.input.clear(); term.cursor = 0; term.cmd_cursor = None;
    if trimmed.is_empty() { return; }

    // Echo
    let mut echo = FixStr::<LINE_CAP>::new();
    echo.push_str("> ");
    let tlen = trimmed.len().min(LINE_CAP - 3);
    for b in trimmed.as_bytes()[..tlen].iter() { echo.push(*b); }
    term.log(echo.as_str(), COL_CMD_ECHO);

    // Append to command history (circular buffer)
    if term.cmd_count < CMD_HIST_CAP {
        term.cmd_hist[term.cmd_count].clear();
        term.cmd_hist[term.cmd_count].push_str(trimmed);
        term.cmd_count += 1;
    } else {
        for i in 0..CMD_HIST_CAP - 1 { term.cmd_hist[i] = term.cmd_hist[i + 1]; }
        term.cmd_hist[CMD_HIST_CAP - 1].clear();
        term.cmd_hist[CMD_HIST_CAP - 1].push_str(trimmed);
    }

    let (run_str, background) = if trimmed.ends_with(" &") {
        (&trimmed[..trimmed.len() - 2], true)
    } else { (trimmed, false) };

    // Copy run_str before calling mutable methods on term
    let mut cmd_buf = [0u8; 256];
    let cl = run_str.len().min(255);
    cmd_buf[..cl].copy_from_slice(&run_str.as_bytes()[..cl]);
    let cmd = core::str::from_utf8(&cmd_buf[..cl]).unwrap_or("");

    // Pipe?
    if let Some(pipe_pos) = cmd.as_bytes().iter().position(|&b| b == b'|') {
        let left  = cmd[..pipe_pos].trim();
        let right = cmd[pipe_pos + 1..].trim();
        let mut lb = [0u8; 128]; let ll = left.len().min(127);
        let mut rb = [0u8; 128]; let rl = right.len().min(127);
        lb[..ll].copy_from_slice(&left.as_bytes()[..ll]);
        rb[..rl].copy_from_slice(&right.as_bytes()[..rl]);
        run_pipe(term,
                 core::str::from_utf8(&lb[..ll]).unwrap_or(""),
                 core::str::from_utf8(&rb[..rl]).unwrap_or(""));
        return;
    }

    // Split first word (cmd vs args)
    let (bare_cmd, args) = if let Some(i) = cmd.bytes().position(|b| b == b' ') {
        (cmd[..i].trim(), cmd[i + 1..].trim())
    } else { (cmd.trim(), "") };

    if !run_builtin(term, bare_cmd, args) {
        run_external(term, cmd, background);
    }
}
