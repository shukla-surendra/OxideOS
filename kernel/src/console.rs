//! Headless serial console — the no-GUI counterpart of `gui_loop`.
//!
//! Built when the kernel is compiled without the `gui` feature (`make GUI=0`).
//! Runs the userspace shell with the serial port as its terminal:
//!   input   serial RX → stdin ring (the PS/2 keyboard IRQ also feeds it, so
//!           typing into a QEMU window works too)
//!   output  the Write syscall already mirrors every user byte to serial
//!           (`write_console`), so the per-task capture buffers are only
//!           drained here to keep them from filling up
//!   Ctrl+C  SIGINT to every task except the shell
//! The shell is respawned whenever it exits.

use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::scheduler::{self, TaskState};
use crate::kernel::{cpu, keyboard, net, programs, stdin};

const SHELL: &str = "sh";
const CTRL_C: u8 = 0x03;

pub unsafe fn run() -> ! {
    unsafe {
        SERIAL_PORT.write_str("\n=== OxideOS headless console (GUI disabled) ===\n");
        SERIAL_PORT.write_str("Type 'help' for shell commands. Ctrl+C interrupts the running program.\n\n");
    }

    let mut shell_pid = unsafe { spawn_shell() };

    loop {
        unsafe { keyboard::poll(); }

        while let Some(byte) = unsafe { SERIAL_PORT.read_byte() } {
            match byte {
                // Host terminals send CR for Enter and DEL for Backspace;
                // the shell and the PS/2 path expect LF and BS.
                b'\r' => stdin::push(b'\n'),
                0x7F  => stdin::push(8),
                CTRL_C => {
                    stdin::push(CTRL_C);
                    unsafe { interrupt_foreground(shell_pid); }
                }
                b => stdin::push(b),
            }
        }

        if let Some((pid, _exit_code)) = unsafe { scheduler::tick() } {
            if Some(pid) == shell_pid {
                unsafe { SERIAL_PORT.write_str("\n[console] shell exited — restarting\n"); }
                shell_pid = unsafe { spawn_shell() };
            }
        }

        for idx in 0..scheduler::MAX_TASKS {
            scheduler::output_drain_task(idx, |_| {});
        }

        unsafe { net::poll(); }
        cpu::wait_for_interrupt();
    }
}

unsafe fn spawn_shell() -> Option<u8> {
    let result = match programs::find(SHELL) {
        Some(code) => unsafe { scheduler::spawn(code, SHELL) },
        None       => Err("not embedded in this build"),
    };
    match result {
        Ok(pid) => Some(pid),
        Err(e) => {
            unsafe {
                SERIAL_PORT.write_str("[console] cannot start shell: ");
                SERIAL_PORT.write_str(e);
                SERIAL_PORT.write_str("\n");
            }
            None
        }
    }
}

/// With a single console every live non-shell task is effectively in the
/// foreground, so Ctrl+C interrupts all of them and leaves the shell alone.
unsafe fn interrupt_foreground(shell_pid: Option<u8>) {
    for task in scheduler::task_infos().iter() {
        let alive = !matches!(task.state, TaskState::Empty | TaskState::Dead(_));
        if alive && task.pid != 0 && Some(task.pid) != shell_pid {
            unsafe { scheduler::send_signal(task.pid, scheduler::SIGINT); }
        }
    }
}
