//! Network connectivity probe — self-contained state machine.
//!
//! Opens a TCP connection to `PROBE_IP:PROBE_PORT` in non-blocking style and
//! updates its phase each GUI frame.  The sysinfo panel reads `phase` to show
//! the animated status and button.

use crate::gui::window_manager::WindowManager;

const PROBE_IP:            [u8; 4] = [93, 184, 216, 34]; // example.com
const PROBE_PORT:          u16      = 80;
const PROBE_TIMEOUT_TICKS: u64      = 1000; // 10 s at 100 Hz

// ── Probe state ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum ProbeFailReason { NoSocket, NoConnect, Timeout }

#[derive(Clone, Copy)]
pub enum NetProbePhase {
    Idle,
    Connecting { sfd: i64, start_tick: u64 },
    Connected  { ms: u64 },
    Failed     { reason: ProbeFailReason },
}

pub struct NetProbe {
    pub phase: NetProbePhase,
}

impl NetProbe {
    pub const fn new() -> Self { Self { phase: NetProbePhase::Idle } }

    /// Advance the state machine one tick.  Returns `true` if the phase changed.
    pub fn tick(&mut self) -> bool {
        if let NetProbePhase::Connecting { sfd, start_tick } = self.phase {
            let now = unsafe { crate::kernel::timer::get_ticks() };
            if now.wrapping_sub(start_tick) > PROBE_TIMEOUT_TICKS {
                unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
                unsafe { crate::kernel::serial::SERIAL_PORT
                    .write_str("[probe] timed out\n"); }
                self.phase = NetProbePhase::Failed { reason: ProbeFailReason::Timeout };
                return true;
            }
            unsafe { crate::kernel::net::poll(); }
            if unsafe { crate::kernel::net::socket::tcp_is_connected(sfd) } {
                let ms = now.wrapping_sub(start_tick) * 10;
                unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
                self.phase = NetProbePhase::Connected { ms };
                return true;
            }
        }
        false
    }

    /// Initiate a TCP connection to the probe target.
    pub fn start(&mut self) {
        if let NetProbePhase::Connecting { sfd, .. } = self.phase {
            unsafe { crate::kernel::net::socket::sys_close_socket(sfd); }
        }

        use crate::kernel::net::socket::{sys_socket, sys_connect, AF_INET, SOCK_STREAM};
        let sfd = unsafe { sys_socket(AF_INET, SOCK_STREAM, 0) };
        if sfd < 0 {
            unsafe { crate::kernel::serial::SERIAL_PORT
                .write_str("[probe] sys_socket failed\n"); }
            self.phase = NetProbePhase::Failed { reason: ProbeFailReason::NoSocket };
            return;
        }

        let mut addr = [0u8; 8];
        addr[0..2].copy_from_slice(&(AF_INET as u16).to_le_bytes());
        addr[2..4].copy_from_slice(&PROBE_PORT.to_be_bytes());
        addr[4..8].copy_from_slice(&PROBE_IP);

        unsafe { crate::kernel::serial::SERIAL_PORT
            .write_str("[probe] connecting to 93.184.216.34:80...\n"); }

        let r = unsafe { sys_connect(sfd, addr.as_ptr(), 8) };
        if r < 0 {
            unsafe {
                crate::kernel::net::socket::sys_close_socket(sfd);
            }
            self.phase = NetProbePhase::Failed { reason: ProbeFailReason::NoConnect };
            return;
        }

        let start_tick = unsafe { crate::kernel::timer::get_ticks() };
        self.phase = NetProbePhase::Connecting { sfd, start_tick };
    }

    /// Returns `true` if `(mx, my)` lands on the "Test" button in the sysinfo window.
    pub fn is_button_hit(&self, wm: &WindowManager, sysinfo_id: usize, mx: u64, my: u64) -> bool {
        let Some(win) = wm.get_window(sysinfo_id) else { return false; };
        if !wm.is_window_visible(sysinfo_id) { return false; }
        let btn_x = win.x + 12;
        let btn_y = win.y + 260; // matches draw_sysinfo_panel layout
        let btn_w = win.width.saturating_sub(24);
        mx >= btn_x && mx < btn_x + btn_w && my >= btn_y && my < btn_y + 22
    }
}
