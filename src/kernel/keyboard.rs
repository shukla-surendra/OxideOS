// OLD: use crate::io::in8;
use super::io::in8;


/// Status register port for keyboard controller (0x64).
const KBD_STATUS: u16 = 0x64;
/// Data port for scancodes (0x60).
const KBD_DATA:   u16 = 0x60;

/// Bit 0 in status: output buffer full (scancode ready).
#[inline]
fn has_data() -> bool {
    (in8(KBD_STATUS) & 1) != 0
}

/// Non-blocking read: returns Some(scancode) if available, else None.
pub fn read_scancode_nonblock() -> Option<u8> {
    if has_data() {
        Some(in8(KBD_DATA))
    } else {
        None
    }
}
