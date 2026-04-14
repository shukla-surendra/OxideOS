// src/kernel/keyboard.rs
//! PS/2 keyboard driver using the `pc-keyboard` crate for robust scancode
//! decoding.  Handles VirtualBox quirks (AUXB bit 5 incorrect for IRQ1).

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use crate::kernel::serial::SERIAL_PORT;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, KeyCode, KeyState, ScancodeSet1};

const KEYBOARD_DEBUG_LOGGING: bool = false;

// ============================================================================
// PC-KEYBOARD DECODER  (scancode-set-1, US QWERTY)
// ============================================================================

/// The pc-keyboard state machine.  Initialised in `init()`.
/// `const fn new` means we can store it as a static directly.
static mut KB: Option<Keyboard<layouts::Us104Key, ScancodeSet1>> = None;

/// Modifier state tracked independently (pc-keyboard 0.7 has no public accessor).
#[derive(Clone, Copy, Default)]
struct ModState {
    lshift:  bool,
    rshift:  bool,
    lctrl:   bool,
    rctrl:   bool,
    lalt:    bool,
    ralt:    bool,
    caps:    bool,
    num:     bool,
    scroll:  bool,
}

static mut MODS: ModState = ModState {
    lshift: false, rshift: false,
    lctrl: false,  rctrl: false,
    lalt: false,   ralt: false,
    caps: false, num: true, scroll: false, // num-lock on by default
};

/// Last LED byte sent to the keyboard (scroll=bit0, num=bit1, caps=bit2).
static LAST_LED: AtomicU8 = AtomicU8::new(0x02); // num-lock on by default

// ============================================================================
// CALLBACK SYSTEM
// ============================================================================

pub type KeyCallback      = unsafe fn(u8);
pub type ArrowKeyCallback = unsafe fn(ArrowKey);

pub enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
}

static mut KEY_CALLBACK:       Option<KeyCallback>      = None;
static mut ARROW_KEY_CALLBACK: Option<ArrowKeyCallback> = None;
static CALLBACK_ENABLED:       AtomicBool               = AtomicBool::new(false);
static ARROW_CALLBACK_ENABLED: AtomicBool               = AtomicBool::new(false);

pub unsafe fn register_key_callback(callback: KeyCallback) {
    unsafe { KEY_CALLBACK = Some(callback); }
    CALLBACK_ENABLED.store(true, Ordering::Relaxed);
}

pub unsafe fn register_arrow_key_callback(callback: ArrowKeyCallback) {
    unsafe { ARROW_KEY_CALLBACK = Some(callback); }
    ARROW_CALLBACK_ENABLED.store(true, Ordering::Relaxed);
}

// ============================================================================
// INTERRUPT / POLL ENTRY POINTS
// ============================================================================

/// Handle keyboard interrupt (IRQ1).
///
/// VirtualBox note: we are in the keyboard IRQ handler (IRQ1), so any data
/// present in the output buffer here is keyboard data — even if bit 5 of the
/// status register says otherwise (VirtualBox sometimes sets it incorrectly).
/// We therefore only gate on OBF (bit 0) and trust that IRQ1 means keyboard.
pub unsafe fn handle_keyboard_interrupt() {
    unsafe {
        let status: u8;
        asm!("in al, 0x64", out("al") status, options(nostack, nomem));
        let scancode: u8;
        asm!("in al, 0x60", out("al") scancode, options(nostack, nomem));

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
}

/// Polling fallback: read any pending keyboard byte without waiting for IRQ.
/// Called from the main GUI loop each frame.
/// In polling context we honour AUXB (bit 5) so we don't consume mouse data.
pub unsafe fn poll() {
    unsafe {
        for _ in 0..8u8 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x01) == 0 { break; }
            let scancode: u8;
            asm!("in al, 0x60", out("al") scancode, options(nostack, nomem));
            if (status & 0x20) == 0 {
                if KEYBOARD_DEBUG_LOGGING {
                    SERIAL_PORT.write_str("[KBD-POLL sc=0x");
                    SERIAL_PORT.write_hex(scancode as u32);
                    SERIAL_PORT.write_str("]\n");
                }
                process_scancode(scancode);
            }
        }
    }
}

// ============================================================================
// SCANCODE PROCESSING  (via pc-keyboard crate)
// ============================================================================

unsafe fn process_scancode(scancode: u8) {
    unsafe {
        let kb = match core::ptr::addr_of_mut!(KB).as_mut().and_then(|o| o.as_mut()) {
            Some(kb) => kb,
            None => return,
        };

        match kb.add_byte(scancode) {
            Ok(Some(key_event)) => {
                // Update our local modifier tracking from the raw event.
                update_modifiers(key_event.code, key_event.state);

                if let Some(decoded) = kb.process_keyevent(key_event) {
                    match decoded {
                        DecodedKey::Unicode(c) => dispatch_unicode(c),
                        DecodedKey::RawKey(kc) => dispatch_raw_key(kc),
                    }
                }
            }
            Ok(None) => {} // incomplete multi-byte sequence — nothing yet
            Err(_)   => {} // bad scancode — ignore
        }
    }
}

/// Update our shadow modifier state and refresh LEDs when lock keys toggle.
unsafe fn update_modifiers(code: KeyCode, state: KeyState) {
    unsafe {
        let m = core::ptr::addr_of_mut!(MODS).as_mut().unwrap();
        let down = state == KeyState::Down;
        match code {
            KeyCode::LShift     => m.lshift = down,
            KeyCode::RShift     => m.rshift = down,
            KeyCode::LControl   => m.lctrl  = down,
            KeyCode::RControl   => m.rctrl  = down,
            KeyCode::LAlt       => m.lalt   = down,
            KeyCode::RAltGr     => m.ralt   = down,
            // Toggle on key-down only
            KeyCode::CapsLock   if down => { m.caps   = !m.caps;   update_leds(m); }
            KeyCode::NumpadLock if down => { m.num    = !m.num;    update_leds(m); }
            KeyCode::ScrollLock if down => { m.scroll = !m.scroll; update_leds(m); }
            _ => {}
        }
    }
}

/// Send updated LED state to the keyboard if it changed.
unsafe fn update_leds(m: &ModState) {
    let led: u8 =
        (if m.scroll { 0x01 } else { 0 }) |
        (if m.num    { 0x02 } else { 0 }) |
        (if m.caps   { 0x04 } else { 0 });

    let prev = LAST_LED.load(Ordering::Relaxed);
    if led != prev {
        LAST_LED.store(led, Ordering::Relaxed);
        unsafe { send_led_command(led); }
    }
}

/// Handle a decoded Unicode character.
unsafe fn dispatch_unicode(c: char) {
    let byte: u8 = match c as u32 {
        0x08 => 8,              // Backspace
        0x09 => b'\t',          // Tab
        0x0A | 0x0D => b'\n',   // Enter / CR → newline
        // Control chars (Ctrl+A..Z) — pass through directly (e.g. 0x03 = Ctrl+C)
        n @ 0x01..=0x1F => n as u8,
        // Printable ASCII
        n @ 0x20..=0x7E => n as u8,
        // Drop anything outside ASCII for now
        _ => return,
    };

    if KEYBOARD_DEBUG_LOGGING {
        unsafe {
            SERIAL_PORT.write_str("[CHAR 0x");
            SERIAL_PORT.write_hex(byte as u32);
            SERIAL_PORT.write_str("]\n");
        }
    }

    unsafe {
        crate::kernel::stdin::push(byte);

        if CALLBACK_ENABLED.load(Ordering::Relaxed) {
            if let Some(cb) = KEY_CALLBACK {
                cb(byte);
            }
        }
    }
}

/// Handle a raw (non-Unicode) key such as arrow keys, F-keys, etc.
unsafe fn dispatch_raw_key(kc: KeyCode) {
    if KEYBOARD_DEBUG_LOGGING {
        unsafe { SERIAL_PORT.write_str("[RAWKEY]\n"); }
    }

    // Some raw keys have ASCII equivalents
    let ascii: Option<u8> = match kc {
        KeyCode::Backspace => Some(8),
        KeyCode::Tab       => Some(b'\t'),
        KeyCode::Return    => Some(b'\n'),
        KeyCode::Escape    => Some(0x1B),
        _                  => None,
    };

    if let Some(byte) = ascii {
        unsafe {
            crate::kernel::stdin::push(byte);
            if CALLBACK_ENABLED.load(Ordering::Relaxed) {
                if let Some(cb) = KEY_CALLBACK {
                    cb(byte);
                }
            }
        }
        return;
    }

    // Navigation keys → arrow callback
    if ARROW_CALLBACK_ENABLED.load(Ordering::Relaxed) {
        unsafe {
            if let Some(cb) = ARROW_KEY_CALLBACK {
                let arrow = match kc {
                    KeyCode::ArrowUp    => Some(ArrowKey::Up),
                    KeyCode::ArrowDown  => Some(ArrowKey::Down),
                    KeyCode::ArrowLeft  => Some(ArrowKey::Left),
                    KeyCode::ArrowRight => Some(ArrowKey::Right),
                    KeyCode::PageUp     => Some(ArrowKey::PageUp),
                    KeyCode::PageDown   => Some(ArrowKey::PageDown),
                    _                   => None,
                };
                if let Some(a) = arrow {
                    cb(a);
                }
            }
        }
    }
}

// ============================================================================
// PUBLIC QUERY API  (kept for compatibility)
// ============================================================================

pub unsafe fn is_shift_pressed() -> bool {
    unsafe { let m = &*core::ptr::addr_of!(MODS); m.lshift || m.rshift }
}

pub unsafe fn is_ctrl_pressed() -> bool {
    unsafe { let m = &*core::ptr::addr_of!(MODS); m.lctrl || m.rctrl }
}

pub unsafe fn is_alt_pressed() -> bool {
    unsafe { let m = &*core::ptr::addr_of!(MODS); m.lalt || m.ralt }
}

pub unsafe fn is_caps_lock_on() -> bool {
    unsafe { (*core::ptr::addr_of!(MODS)).caps }
}

// ============================================================================
// 8042 CONTROLLER HELPERS
// ============================================================================

unsafe fn wait_write_ready() {
    unsafe {
        for _ in 0..100_000u32 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x02) == 0 { return; }
            asm!("pause", options(nostack, nomem));
        }
    }
}

unsafe fn flush_output_buffer() {
    unsafe {
        for _ in 0..16u8 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x01) == 0 { break; }
            let _: u8;
            asm!("in al, 0x60", out("al") _, options(nostack, nomem));
        }
    }
}

unsafe fn ctrl_cmd(cmd: u8) {
    unsafe {
        wait_write_ready();
        asm!("out 0x64, al", in("al") cmd, options(nostack, nomem));
    }
}

unsafe fn ctrl_data(b: u8) {
    unsafe {
        wait_write_ready();
        asm!("out 0x60, al", in("al") b, options(nostack, nomem));
    }
}

/// Read one byte from the 8042 with a short timeout.  Returns 0xFF on timeout.
unsafe fn ctrl_read_fast() -> u8 {
    unsafe {
        for _ in 0..10_000u32 {
            let status: u8;
            asm!("in al, 0x64", out("al") status, options(nostack, nomem));
            if (status & 0x01) != 0 {
                let v: u8;
                asm!("in al, 0x60", out("al") v, options(nostack, nomem));
                return v;
            }
            asm!("pause", options(nostack, nomem));
        }
        0xFF
    }
}

/// Send LED command to keyboard (best-effort; timeout-safe).
unsafe fn send_led_command(led: u8) {
    unsafe {
        ctrl_data(0xED);
        ctrl_read_fast(); // consume ACK
        ctrl_data(led);
        ctrl_read_fast(); // consume ACK
    }
}

// ============================================================================
// INITIALISATION
// ============================================================================

/// Initialise the keyboard driver.
///
/// Strategy for maximum VirtualBox / QEMU compatibility:
///  - Drain stale output-buffer bytes.
///  - Write a known-good CCB directly (no CCB read — unreliable on some hypervisors).
///    CCB = 0x47: IRQ1 enabled (bit 0), IRQ12 enabled (bit 1), scancode
///    translation enabled (bit 6), both clocks enabled (bits 4/5 = 0).
///  - Re-enable the keyboard port.
///  - Set LEDs.
pub unsafe fn init() {
    unsafe {
        SERIAL_PORT.write_str("Initializing keyboard driver (pc-keyboard + 8042)...\n");

        // Set up the pc-keyboard decoder (scancode set 1, US layout).
        // HandleControl::MapLettersToUnicode: Ctrl+C → '\x03', Ctrl+D → '\x04', etc.
        *core::ptr::addr_of_mut!(KB) = Some(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::MapLettersToUnicode,
        ));

        // 1. Drain any stale data.
        flush_output_buffer();

        // 2. Disable keyboard port temporarily.
        ctrl_cmd(0xAD);

        // 3. Drain again.
        flush_output_buffer();

        // 4. Write a known-good CCB without reading the old one.
        ctrl_cmd(0x60);
        ctrl_data(0x47);

        // 5. Re-enable the keyboard port.
        ctrl_cmd(0xAE);

        // 6. Drain once more.
        flush_output_buffer();

        // 7. Set initial LEDs (num-lock on).
        send_led_command(0x02);

        SERIAL_PORT.write_str("Keyboard driver ready\n");
    }
}
