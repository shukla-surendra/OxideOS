// src/kernel/keyboard.rs
//! Keyboard driver with full scancode to ASCII translation
//! Supports US QWERTY layout with shift, caps lock, and control states
//! Rust 2024 Edition compatible

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;
use core::sync::atomic::{AtomicBool, Ordering};

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

/// Handle keyboard interrupt (IRQ1)
pub unsafe fn handle_keyboard_interrupt() {
    // Check status register to verify keyboard data
    let status: u8;
    asm!("in al, 0x64", out("al") status, options(nostack, nomem));

    // Check if data is available and it's from keyboard (not mouse)
    if (status & 0x01) != 0 && (status & 0x20) == 0 {
        // Read scancode
        let scancode: u8;
        asm!("in al, 0x60", out("al") scancode, options(nostack, nomem));
        
        // Process the scancode
        process_scancode(scancode);
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
                unsafe {
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
                unsafe {
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
            unsafe { SERIAL_PORT.write_str("[ESC]\n") };
            return;
        }
        SC_BACKSPACE => {
            unsafe { SERIAL_PORT.write_str("[BACKSPACE]\n") };
            handle_backspace();
            // Call callback with backspace
            if CALLBACK_ENABLED.load(Ordering::Relaxed) {
                if let Some(callback) = unsafe { KEY_CALLBACK } {
                    callback(8);
                }
            }
            return;
        }
        SC_TAB => {
            unsafe { SERIAL_PORT.write_str("[TAB]\n") };
            if CALLBACK_ENABLED.load(Ordering::Relaxed) {
                if let Some(callback) = unsafe { KEY_CALLBACK } {
                    callback(b'\t');
                }
            }
            return;
        }
        SC_ENTER => {
            unsafe { SERIAL_PORT.write_str("[ENTER]\n") };
            handle_enter();
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
                        unsafe { SERIAL_PORT.write_str("[UP]\n") };
                        callback(ArrowKey::Up);
                    }
                    SC_DOWN => {
                        unsafe { SERIAL_PORT.write_str("[DOWN]\n") };
                        callback(ArrowKey::Down);
                    }
                    SC_LEFT => {
                        unsafe { SERIAL_PORT.write_str("[LEFT]\n") };
                        callback(ArrowKey::Left);
                    }
                    SC_RIGHT => {
                        unsafe { SERIAL_PORT.write_str("[RIGHT]\n") };
                        callback(ArrowKey::Right);
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
        
        // Echo to serial
        unsafe { SERIAL_PORT.write_byte(ch) };
        
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
    
    // Process the input buffer (for shell/command line)
    unsafe {
        SERIAL_PORT.write_str("\nInput: ");
        for &ch in state.get_buffer() {
            SERIAL_PORT.write_byte(ch);
        }
        SERIAL_PORT.write_str("\n");
    }
    
    // Clear buffer for next input
    state.clear_buffer();
}

/// Update keyboard LEDs based on current state
unsafe fn update_keyboard_leds() {
    let state = unsafe { &*core::ptr::addr_of!(KEYBOARD_STATE) };
    
    let mut led_state: u8 = 0;
    if state.scroll_lock { led_state |= 0x01; }
    if state.num_lock    { led_state |= 0x02; }
    if state.caps_lock   { led_state |= 0x04; }
    
    // Wait for keyboard to be ready
    wait_for_keyboard();
    
    // Send LED update command
    unsafe {
        asm!("out 0x60, al", in("al") 0xEDu8, options(nostack, nomem));
    }
    
    wait_for_keyboard();
    
    // Send LED state
    unsafe {
        asm!("out 0x60, al", in("al") led_state, options(nostack, nomem));
    }
    
    wait_for_keyboard();
}

/// Wait for keyboard controller to be ready
unsafe fn wait_for_keyboard() {
    for _ in 0..1000 {
        let status: u8;
        unsafe {
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
        }
        if (status & 0x02) == 0 {
            return;
        }
        // Small delay
        for _ in 0..100 {
            core::arch::asm!("pause");
        }
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

/// Initialize keyboard driver
pub unsafe fn init() {
    unsafe {
        SERIAL_PORT.write_str("Initializing keyboard driver...\n");
        
        // Set default LED state
        update_keyboard_leds();
        
        SERIAL_PORT.write_str("Keyboard driver ready\n");
    }
}