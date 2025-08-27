/// Minimal decoded key set for simplicity.
pub enum DecodedKey {
    Ascii(u8),
    Enter,
    Backspace,
    None, // releases/unsupported keys
}

/// Translate a Set-1 *make* scancode to a DecodedKey.
/// - Ignores break codes (>= 0x80) and modifiers (Shift/Caps) for now.
pub fn decode_scancode(sc: u8) -> DecodedKey {
    // Ignore key releases
    if sc & 0x80 != 0 {
        return DecodedKey::None;
    }

    // Special keys
    match sc {
        0x1C => return DecodedKey::Enter,     // Enter
        0x0E => return DecodedKey::Backspace, // Backspace
        0x39 => return DecodedKey::Ascii(b' '), // Space
        _ => {}
    }

    // Top row digits (no shift)
    if let Some(ch) = match sc {
        0x02 => Some(b'1'), 0x03 => Some(b'2'), 0x04 => Some(b'3'), 0x05 => Some(b'4'),
        0x06 => Some(b'5'), 0x07 => Some(b'6'), 0x08 => Some(b'7'), 0x09 => Some(b'8'),
        0x0A => Some(b'9'), 0x0B => Some(b'0'), 0x0C => Some(b'-'), 0x0D => Some(b'='),
        _ => None,
    } { return DecodedKey::Ascii(ch); }

    // Letters (lowercase)
    if let Some(ch) = match sc {
        0x10 => Some(b'q'), 0x11 => Some(b'w'), 0x12 => Some(b'e'), 0x13 => Some(b'r'),
        0x14 => Some(b't'), 0x15 => Some(b'y'), 0x16 => Some(b'u'), 0x17 => Some(b'i'),
        0x18 => Some(b'o'), 0x19 => Some(b'p'),
        0x1E => Some(b'a'), 0x1F => Some(b's'), 0x20 => Some(b'd'), 0x21 => Some(b'f'),
        0x22 => Some(b'g'), 0x23 => Some(b'h'), 0x24 => Some(b'j'), 0x25 => Some(b'k'),
        0x26 => Some(b'l'),
        0x2C => Some(b'z'), 0x2D => Some(b'x'), 0x2E => Some(b'c'), 0x2F => Some(b'v'),
        0x30 => Some(b'b'), 0x31 => Some(b'n'), 0x32 => Some(b'm'),
        _ => None,
    } { return DecodedKey::Ascii(ch); }

    // Punctuation
    if let Some(ch) = match sc {
        0x1A => Some(b'['), 0x1B => Some(b']'),
        0x27 => Some(b';'), 0x28 => Some(b'\''), 0x29 => Some(b'`'),
        0x2B => Some(b'\\'),
        0x33 => Some(b','), 0x34 => Some(b'.'), 0x35 => Some(b'/'),
        _ => None,
    } { return DecodedKey::Ascii(ch); }

    DecodedKey::None
}
