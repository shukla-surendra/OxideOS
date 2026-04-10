// src/kernel/keyboard.rs
//! Keyboard driver with full scancode to ASCII translation
//! Supports US QWERTY layout with shift, caps lock, and control states
//! Rust 2024 Edition compatible

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;
use core::sync::atomic::{AtomicBool, Ordering};

const KEYBOARD_DEBUG_LOGGING: bool = true; // ← serial log every scancode for VirtualBox debug

// ============================================================================
// KEYBOARD STATE
// ============================================================================

static mut KEYBOARD_STATE: KeyboardState = KeyboardState::new();

struct KeyboardState {
    shift_pressed: bool,
    ctrl_pressed: bool,
    alt_pressed: bool,
    caps_lock: bool,
    num_lock: bool,
    scroll_lock: bool,
    
    // Extended scancode tracking
    extended_code: bool,
    
    // Input buffer for shell/applications
    input_buffer: [u8; 256],
    buffer_pos: usize,
}

impl KeyboardState {
    const fn new() -> Self {
        Self {
            shift_pressed: false,
            ctrl_pressed: false,
            alt_pressed: false,
            caps_lock: false,
            num_lock: true,  // NumLock on by default
            scroll_lock: false,
            extended_code: false,
            input_buffer: [0; 256],
            buffer_pos: 0,
        }
    }
    
    fn add_to_buffer(&mut self, ch: u8) {
        if self.buffer_pos < 255 {
            self.input_buffer[self.buffer_pos] = ch;
            self.buffer_pos += 1;
        }
    }
    
    fn get_buffer(&self) -> &[u8] {
        &self.input_buffer[..self.buffer_pos]
    }
    
    fn clear_buffer(&mut self) {
        self.buffer_pos = 0;
    }
}

// ============================================================================
// SCANCODE TABLES
// ============================================================================

// US QWERTY keyboard layout - Set 1 scancodes
const SCANCODE_TO_ASCII: [u8; 128] = [
    0,    // 0x00 - Error
    0,    // 0x01 - Escape
    b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0',  // 0x02-0x0B
    b'-', b'=',  // 0x0C-0x0D
    0,    // 0x0E - Backspace
    0,    // 0x0F - Tab
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p',  // 0x10-0x19
    b'[', b']',  // 0x1A-0x1B
    0,    // 0x1C - Enter
    0,    // 0x1D - Left Ctrl
    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l',  // 0x1E-0x26
    b';', b'\'', b'`',  // 0x27-0x29
    0,    // 0x2A - Left Shift
    b'\\',  // 0x2B
    b'z', b'x', b'c', b'v', b'b', b'n', b'm',  // 0x2C-0x32
    b',', b'.', b'/',  // 0x33-0x35
    0,    // 0x36 - Right Shift
    b'*',  // 0x37 - Keypad *
    0,    // 0x38 - Left Alt
    b' ',  // 0x39 - Space
    0,    // 0x3A - Caps Lock
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,  // 0x3B-0x44 - F1-F10
    0,    // 0x45 - Num Lock
    0,    // 0x46 - Scroll Lock
    b'7', b'8', b'9', b'-',  // 0x47-0x4A - Keypad
    b'4', b'5', b'6', b'+',  // 0x4B-0x4E - Keypad
    b'1', b'2', b'3',        // 0x4F-0x51 - Keypad
    b'0', b'.',              // 0x52-0x53 - Keypad
    0, 0, 0,                 // 0x54-0x56
    0, 0,                    // 0x57-0x58 - F11-F12
    // Rest are zeros (0x59-0x7F = 39 more entries)
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// Shifted characters
const SCANCODE_TO_ASCII_SHIFT: [u8; 128] = [
    0,    // 0x00 - Error
    0,    // 0x01 - Escape
    b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')',  // 0x02-0x0B
    b'_', b'+',  // 0x0C-0x0D
    0,    // 0x0E - Backspace
    0,    // 0x0F - Tab
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P',  // 0x10-0x19
    b'{', b'}',  // 0x1A-0x1B
    0,    // 0x1C - Enter
    0,    // 0x1D - Left Ctrl
    b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L',  // 0x1E-0x26
    b':', b'"', b'~',  // 0x27-0x29
    0,    // 0x2A - Left Shift
    b'|',  // 0x2B
    b'Z', b'X', b'C', b'V', b'B', b'N', b'M',  // 0x2C-0x32
    b'<', b'>', b'?',  // 0x33-0x35
    0,    // 0x36 - Right Shift
    b'*',  // 0x37 - Keypad *
    0,    // 0x38 - Left Alt
    b' ',  // 0x39 - Space
    0,    // 0x3A - Caps Lock
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,  // 0x3B-0x44 - F1-F10
    0,    // 0x45 - Num Lock
    0,    // 0x46 - Scroll Lock
    b'7', b'8', b'9', b'-',  // 0x47-0x4A - Keypad
    b'4', b'5', b'6', b'+',  // 0x4B-0x4E - Keypad
    b'1', b'2', b'3',        // 0x4F-0x51 - Keypad
    b'0', b'.',              // 0x52-0x53 - Keypad
    0, 0, 0,                 // 0x54-0x56
    0, 0,                    // 0x57-0x58 - F11-F12
    // Rest are zeros (0x59-0x7F = 39 more entries)
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// ============================================================================
// SPECIAL SCANCODES
// ============================================================================

const SC_ESCAPE: u8       = 0x01;
const SC_BACKSPACE: u8    = 0x0E;
const SC_TAB: u8          = 0x0F;
const SC_ENTER: u8        = 0x1C;
const SC_LCTRL: u8        = 0x1D;
const SC_LSHIFT: u8       = 0x2A;
const SC_RSHIFT: u8       = 0x36;
const SC_LALT: u8         = 0x38;
const SC_CAPSLOCK: u8     = 0x3A;
const SC_NUMLOCK: u8      = 0x45;
const SC_SCROLLLOCK: u8   = 0x46;

// Extended scancodes (prefixed with 0xE0)
const SC_EXTENDED: u8     = 0xE0;
const SC_RCTRL: u8        = 0x1D;  // After 0xE0
const SC_RALT: u8         = 0x38;  // After 0xE0

// Arrow keys (extended)
const SC_UP: u8           = 0x48;
const SC_DOWN: u8         = 0x50;
const SC_LEFT: u8         = 0x4B;
const SC_RIGHT: u8        = 0x4D;
const SC_PAGE_UP: u8      = 0x49;
const SC_PAGE_DOWN: u8    = 0x51;

// ============================================================================
// CALLBACK SYSTEM
// ============================================================================

pub type KeyCallback = unsafe fn(u8);
pub type ArrowKeyCallback = unsafe fn(ArrowKey);

pub enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
}

static mut KEY_CALLBACK: Option<KeyCallback> = None;
static mut ARROW_KEY_CALLBACK: Option<ArrowKeyCallback> = None;
static CALLBACK_ENABLED: AtomicBool = AtomicBool::new(false);
static ARROW_CALLBACK_ENABLED: AtomicBool = AtomicBool::new(false);

pub unsafe fn register_key_callback(callback: KeyCallback) {
    unsafe {
        KEY_CALLBACK = Some(callback);
    }
    CALLBACK_ENABLED.store(true, Ordering::Relaxed);
}

pub unsafe fn register_arrow_key_callback(callback: ArrowKeyCallback) {
    unsafe {
        ARROW_KEY_CALLBACK = Some(callback);
    }
    ARROW_CALLBACK_ENABLED.store(true, Ordering::Relaxed);
}

// ============================================================================
// KEYBOARD INTERRUPT HANDLER
// ============================================================================

/// Handle keyboard interrupt (IRQ1).
///
/// VirtualBox note: we are in the keyboard IRQ handler (IRQ1), so any data
/// present in the output buffer here is keyboard data — even if bit 5 of the
/// status register says otherwise (VirtualBox sometimes sets it incorrectly).
/// We therefore only gate on OBF (bit 0) and trust that IRQ1 means keyboard.
pub unsafe fn handle_keyboard_interrupt() {
    let status: u8;
    unsafe { asm!("in al, 0x64", out("al") status, options(nostack, nomem)); }

    // Always read port 0x60 if OBF is set — in IRQ1 context the data IS keyboard.
    // Do NOT gate on bit 5 (AUXB): VirtualBox incorrectly sets it for keyboard data.
    let scancode: u8;
    unsafe { asm!("in al, 0x60", out("al") scancode, options(nostack, nomem)); }

    if KEYBOARD_DEBUG_LOGGING {
        SERIAL_PORT.write_str("[KBD-IRQ sc=0x");
        SERIAL_PORT.write_hex(scancode as u32);
        SERIAL_PORT.write_str(" st=0x");
        SERIAL_PORT.write_hex(status as u32);
        SERIAL_PORT.write_str("]\n");
    }

    if (status & 0x01) != 0 {
        process_scancode(scancode);
    }
}

/// Polling fallback: read any pending keyboard byte without waiting for IRQ.
/// Called from the main GUI loop each frame — handles VirtualBox/firmware
/// setups where IRQ1 is unreliable.
///
/// We do NOT check bit 5 (AUXB) here because VirtualBox incorrectly sets it
/// for keyboard data, causing all scancodes to be silently skipped.
/// Mouse data is already consumed by IRQ12 before we reach this point.
pub unsafe fn poll() {
    for _ in 0..8u8 {
        let status: u8;
        unsafe { asm!("in al, 0x64", out("al") status, options(nostack, nomem)); }
        if (status & 0x01) == 0 { break; } // output buffer empty — nothing to read
        let scancode: u8;
        unsafe { asm!("in al, 0x60", out("al") scancode, options(nostack, nomem)); }
        // Skip if this is mouse data (bit 5 set) — mouse bytes come via IRQ12.
        // VirtualBox exception: in IRQ1 context we trust the IRQ number, but here
        // in a polling context we honour AUXB since mouse and keyboard share port 0x60.
        // If this causes issues on specific VirtualBox versions, remove the bit-5 guard.
        if (status & 0x20) == 0 {
            if KEYBOARD_DEBUG_LOGGING {
                SERIAL_PORT.write_str("[KBD-POLL sc=0x");
                SERIAL_PORT.write_hex(scancode as u32);
                SERIAL_PORT.write_str(" st=0x");
                SERIAL_PORT.write_hex(status as u32);
                SERIAL_PORT.write_str("]\n");
            }
            process_scancode(scancode);
        } else if KEYBOARD_DEBUG_LOGGING {
            SERIAL_PORT.write_str("[KBD-POLL-MOUSE sc=0x");
            SERIAL_PORT.write_hex(scancode as u32);
            SERIAL_PORT.write_str(" st=0x");
            SERIAL_PORT.write_hex(status as u32);
            SERIAL_PORT.write_str("]\n");
        }
    }
}

/// Process a scancode and update keyboard state
unsafe fn process_scancode(scancode: u8) {
    let state = unsafe { &mut *core::ptr::addr_of_mut!(KEYBOARD_STATE) };
    
    // Handle extended scancode prefix
    if scancode == SC_EXTENDED {
        state.extended_code = true;
        return;
    }
    
    let is_extended = state.extended_code;
    state.extended_code = false;
    
    // Check if this is a key release (bit 7 set)
    let is_release = (scancode & 0x80) != 0;
    let scancode = scancode & 0x7F;  // Clear release bit
    
    // Handle modifier keys
    if handle_modifier_keys(scancode, is_release, is_extended) {
        return;  // Was a modifier key, already handled
    }
    
    // Only process key presses (not releases) for regular keys
    if !is_release {
        handle_key_press(scancode, is_extended);
    }
}

/// Handle modifier and special keys
/// Returns true if the scancode was a modifier key
unsafe fn handle_modifier_keys(scancode: u8, is_release: bool, is_extended: bool) -> bool {
    let state = unsafe { &mut *core::ptr::addr_of_mut!(KEYBOARD_STATE) };
    
    match scancode {
        SC_LSHIFT | SC_RSHIFT => {
            state.shift_pressed = !is_release;
            true
        }
        SC_LCTRL => {
            if !is_extended {
                state.ctrl_pressed = !is_release;
            }
            true
        }
        SC_RCTRL => {
            if is_extended {
                state.ctrl_pressed = !is_release;
            }
            true
        }
        SC_LALT => {
            if !is_extended {
                state.alt_pressed = !is_release;
            }
            true
        }
        SC_RALT => {
            if is_extended {
                state.alt_pressed = !is_release;
            }
            true
        }
        SC_CAPSLOCK => {
            if !is_release {
                state.caps_lock = !state.caps_lock;
                update_keyboard_leds();
                if KEYBOARD_DEBUG_LOGGING {
                    SERIAL_PORT.write_str("[CAPS LOCK ");
                    if state.caps_lock {
                        SERIAL_PORT.write_str("ON");
                    } else {
                        SERIAL_PORT.write_str("OFF");
                    }
                    SERIAL_PORT.write_str("]\n");
                }
            }
            true
        }
        SC_NUMLOCK => {
            if !is_release {
                state.num_lock = !state.num_lock;
                update_keyboard_leds();
                if KEYBOARD_DEBUG_LOGGING {
                    SERIAL_PORT.write_str("[NUM LOCK ");
                    if state.num_lock {
                        SERIAL_PORT.write_str("ON");
                    } else {
                        SERIAL_PORT.write_str("OFF");
                    }
                    SERIAL_PORT.write_str("]\n");
                }
            }
            true
        }
        SC_SCROLLLOCK => {
            if !is_release {
                state.scroll_lock = !state.scroll_lock;
                update_keyboard_leds();
            }
            true
        }
        _ => false
    }
}

/// Handle regular key presses and convert to ASCII
unsafe fn handle_key_press(scancode: u8, is_extended: bool) {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    
    // Handle special keys
    match scancode {
        SC_ESCAPE => {
            if KEYBOARD_DEBUG_LOGGING {
                unsafe { SERIAL_PORT.write_str("[ESC]\n") };
            }
            return;
        }
        SC_BACKSPACE => {
            if KEYBOARD_DEBUG_LOGGING {
                unsafe { SERIAL_PORT.write_str("[BACKSPACE]\n") };
            }
            handle_backspace();
            crate::kernel::stdin::push(8);
            // Call callback with backspace
            if CALLBACK_ENABLED.load(Ordering::Relaxed) {
                if let Some(callback) = unsafe { KEY_CALLBACK } {
                    callback(8);
                }
            }
            return;
        }
        SC_TAB => {
            if KEYBOARD_DEBUG_LOGGING {
                unsafe { SERIAL_PORT.write_str("[TAB]\n") };
            }
            crate::kernel::stdin::push(b'\t');
            if CALLBACK_ENABLED.load(Ordering::Relaxed) {
                if let Some(callback) = unsafe { KEY_CALLBACK } {
                    callback(b'\t');
                }
            }
            return;
        }
        SC_ENTER => {
            if KEYBOARD_DEBUG_LOGGING {
                unsafe { SERIAL_PORT.write_str("[ENTER]\n") };
            }
            handle_enter();
            crate::kernel::stdin::push(b'\n');
            // Call callback with newline
            if CALLBACK_ENABLED.load(Ordering::Relaxed) {
                if let Some(callback) = unsafe { KEY_CALLBACK } {
                    callback(b'\n');
                }
            }
            return;
        }
        _ => {}
    }
    
    // Handle extended keys (arrows, etc.)
    if is_extended {
        if ARROW_CALLBACK_ENABLED.load(Ordering::Relaxed) {
            if let Some(callback) = unsafe { ARROW_KEY_CALLBACK } {
                match scancode {
                    SC_UP => {
                        if KEYBOARD_DEBUG_LOGGING {
                            unsafe { SERIAL_PORT.write_str("[UP]\n") };
                        }
                        callback(ArrowKey::Up);
                    }
                    SC_DOWN => {
                        if KEYBOARD_DEBUG_LOGGING {
                            unsafe { SERIAL_PORT.write_str("[DOWN]\n") };
                        }
                        callback(ArrowKey::Down);
                    }
                    SC_LEFT => {
                        if KEYBOARD_DEBUG_LOGGING {
                            unsafe { SERIAL_PORT.write_str("[LEFT]\n") };
                        }
                        callback(ArrowKey::Left);
                    }
                    SC_RIGHT => {
                        if KEYBOARD_DEBUG_LOGGING {
                            unsafe { SERIAL_PORT.write_str("[RIGHT]\n") };
                        }
                        callback(ArrowKey::Right);
                    }
                    SC_PAGE_UP => {
                        callback(ArrowKey::PageUp);
                    }
                    SC_PAGE_DOWN => {
                        callback(ArrowKey::PageDown);
                    }
                    _ => {}
                }
            }
        }
        return;
    }
    
    // Convert scancode to ASCII
    if let Some(ch) = scancode_to_ascii(scancode) {
        // Add to input buffer
        let state = unsafe { &mut *core::ptr::addr_of_mut!(KEYBOARD_STATE) };
        state.add_to_buffer(ch);

        if KEYBOARD_DEBUG_LOGGING {
            unsafe { SERIAL_PORT.write_byte(ch) };
        }

        // Also push into the global stdin ring so user programs can read it.
        crate::kernel::stdin::push(ch);

        // Call registered callback if any
        if CALLBACK_ENABLED.load(Ordering::Relaxed) {
            if let Some(callback) = unsafe { KEY_CALLBACK } {
                callback(ch);
            }
        }
    }
}

/// Convert scancode to ASCII character based on current keyboard state
unsafe fn scancode_to_ascii(scancode: u8) -> Option<u8> {
    if scancode >= 128 {
        return None;
    }
    
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    
    // Determine if we should use shifted table
    let use_shift = state.shift_pressed;
    
    // Get base character
    let mut ch = if use_shift {
        SCANCODE_TO_ASCII_SHIFT[scancode as usize]
    } else {
        SCANCODE_TO_ASCII[scancode as usize]
    };
    
    if ch == 0 {
        return None;
    }
    
    // Apply caps lock to letters
    if state.caps_lock && ch.is_ascii_alphabetic() {
        // Toggle case
        if ch.is_ascii_lowercase() {
            ch = ch.to_ascii_uppercase();
        } else {
            ch = ch.to_ascii_lowercase();
        }
    }
    
    // Handle Ctrl+key combinations
    if state.ctrl_pressed && ch.is_ascii_alphabetic() {
        // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
        ch = (ch.to_ascii_uppercase() - b'A' + 1) & 0x1F;
    }
    
    Some(ch)
}

/// Handle backspace key
unsafe fn handle_backspace() {
    let state = unsafe { &mut *core::ptr::addr_of_mut!(KEYBOARD_STATE) };
    if state.buffer_pos > 0 {
        state.buffer_pos -= 1;
    }
}

/// Handle enter key
unsafe fn handle_enter() {
    let state = unsafe { &mut *core::ptr::addr_of_mut!(KEYBOARD_STATE) };
    
    if KEYBOARD_DEBUG_LOGGING {
        unsafe {
            SERIAL_PORT.write_str("\nInput: ");
            for &ch in state.get_buffer() {
                SERIAL_PORT.write_byte(ch);
            }
            SERIAL_PORT.write_str("\n");
        }
    }
    
    // Clear buffer for next input
    state.clear_buffer();
}

/// Read one byte from the 8042 with a short timeout; returns 0xFF on timeout.
/// Used in LED update so VirtualBox environments that don't ACK don't block long.
unsafe fn ctrl_read_fast() -> u8 {
    for _ in 0..10_000u32 {
        let status: u8;
        unsafe { asm!("in al, 0x64", out("al") status, options(nostack, nomem)); }
        if (status & 0x01) != 0 {
            let v: u8;
            unsafe { asm!("in al, 0x60", out("al") v, options(nostack, nomem)); }
            return v;
        }
        unsafe { asm!("pause", options(nostack, nomem)); }
    }
    0xFF // timeout
}

/// Update keyboard LEDs based on current state.
unsafe fn update_keyboard_leds() {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };

    let mut led_state: u8 = 0;
    if state.scroll_lock { led_state |= 0x01; }
    if state.num_lock    { led_state |= 0x02; }
    if state.caps_lock   { led_state |= 0x04; }

    unsafe {
        ctrl_data(0xED);           // LED command
        ctrl_read_fast();          // consume ACK (0xFA) — timeout-safe
        ctrl_data(led_state);
        ctrl_read_fast();          // consume ACK — timeout-safe
    }
}

// ============================================================================
// PUBLIC API
// ============================================================================

/// Get the current input buffer (useful for shell/command line)
pub unsafe fn get_input_buffer() -> &'static [u8] {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    state.get_buffer()
}

/// Clear the input buffer
pub unsafe fn clear_input_buffer() {
    let state = unsafe { &mut *core::ptr::addr_of_mut!(KEYBOARD_STATE) };
    state.clear_buffer()
}

/// Check if a specific key is currently pressed
pub unsafe fn is_shift_pressed() -> bool {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    state.shift_pressed
}

pub unsafe fn is_ctrl_pressed() -> bool {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    state.ctrl_pressed
}

pub unsafe fn is_alt_pressed() -> bool {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    state.alt_pressed
}

pub unsafe fn is_caps_lock_on() -> bool {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    state.caps_lock
}

// ── 8042 controller helpers ──────────────────────────────────────────────────

/// Wait until the 8042 input buffer is empty (safe to write a command/data).
unsafe fn wait_write_ready() {
    for _ in 0..100_000u32 {
        let status: u8;
        unsafe { asm!("in al, 0x64", out("al") status, options(nostack, nomem)); }
        if (status & 0x02) == 0 { return; }
        unsafe { asm!("pause", options(nostack, nomem)); }
    }
}

/// Wait until the 8042 output buffer has data (safe to read from 0x60).
unsafe fn wait_read_ready() {
    for _ in 0..100_000u32 {
        let status: u8;
        unsafe { asm!("in al, 0x64", out("al") status, options(nostack, nomem)); }
        if (status & 0x01) != 0 { return; }
        unsafe { asm!("pause", options(nostack, nomem)); }
    }
}

/// Drain any stale bytes from the 8042 output buffer.
unsafe fn flush_output_buffer() {
    for _ in 0..16u8 {
        let status: u8;
        unsafe { asm!("in al, 0x64", out("al") status, options(nostack, nomem)); }
        if (status & 0x01) == 0 { break; }
        let _: u8;
        unsafe { asm!("in al, 0x60", out("al") _, options(nostack, nomem)); }
    }
}

/// Send a command byte to the 8042 controller (port 0x64).
unsafe fn ctrl_cmd(cmd: u8) {
    unsafe {
        wait_write_ready();
        asm!("out 0x64, al", in("al") cmd, options(nostack, nomem));
    }
}

/// Write a data byte to the 8042 (port 0x60).
unsafe fn ctrl_data(b: u8) {
    unsafe {
        wait_write_ready();
        asm!("out 0x60, al", in("al") b, options(nostack, nomem));
    }
}

/// Read one byte from the 8042 output buffer (port 0x60).
unsafe fn ctrl_read() -> u8 {
    unsafe {
        wait_read_ready();
        let v: u8;
        asm!("in al, 0x60", out("al") v, options(nostack, nomem));
        v
    }
}

/// Initialize keyboard driver.
///
/// Strategy for maximum VirtualBox / QEMU compatibility:
///   • Drain stale output-buffer bytes first.
///   • Write a known-good CCB directly (no CCB read — reading it is unreliable
///     on some hypervisors and we have been burned by garbage values before).
///   • CCB = 0x47: IRQ1 enabled (bit 0), IRQ12 enabled (bit 1), scancode
///     translation enabled (bit 6), both clocks enabled (bits 4/5 = 0).
///   • Re-enable the keyboard port.
///   • Update LEDs with a short timeout so we don't hang on VirtualBox.
pub unsafe fn init() {
    unsafe {
        SERIAL_PORT.write_str("Initializing keyboard driver (8042)...\n");

        // 1. Drain any stale data.
        flush_output_buffer();

        // 2. Disable keyboard port temporarily.
        ctrl_cmd(0xAD);

        // 3. Drain again in case keyboard queued a byte.
        flush_output_buffer();

        // 4. Write a known-good CCB without reading the old one.
        //    Bit 0 = enable keyboard IRQ1
        //    Bit 1 = enable mouse IRQ12
        //    Bit 6 = enable keyboard scancode translation (set 1 → set 2 xlat)
        //    Bits 4/5 = 0 → both PS/2 clocks enabled
        ctrl_cmd(0x60);
        ctrl_data(0x47);

        // 5. Re-enable the keyboard port.
        ctrl_cmd(0xAE);

        // 6. Drain once more.
        flush_output_buffer();

        // 7. Set keyboard LEDs (best-effort; timeout-safe via ctrl_read_fast).
        update_keyboard_leds();

        SERIAL_PORT.write_str("Keyboard driver ready\n");
    }
}
