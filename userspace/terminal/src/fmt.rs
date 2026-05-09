//! Number-to-string formatting helpers shared by `draw` and `commands`.
//!
//! Using inline buffers keeps every call allocation-free.

/// Format `v` as decimal into `buf` and return the filled slice as `&str`.
pub fn fmt_u32(buf: &mut [u8; 16], v: u32) -> &str {
    let mut i = buf.len();
    let mut vv = v;
    if vv == 0 { i -= 1; buf[i] = b'0'; }
    while vv > 0 { i -= 1; buf[i] = b'0' + (vv % 10) as u8; vv /= 10; }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

/// Format `v` (signed) as decimal into `buf` and return the filled slice as `&str`.
pub fn fmt_i64(buf: &mut [u8; 24], v: i64) -> &str {
    if v >= 0 {
        let mut b16 = [0u8; 16];
        let s   = fmt_u32(&mut b16, v as u32);
        let off = 8;
        let len = s.len();
        buf[off..off + len].copy_from_slice(s.as_bytes());
        return core::str::from_utf8(&buf[off..off + len]).unwrap_or("?");
    }
    let mag = (-(v as i128)) as u64;
    let mut tmp = [0u8; 22]; let mut i = tmp.len();
    let mut vv = mag;
    if vv == 0 { i -= 1; tmp[i] = b'0'; }
    while vv > 0 { i -= 1; tmp[i] = b'0' + (vv % 10) as u8; vv /= 10; }
    i -= 1; tmp[i] = b'-';
    let len = tmp.len() - i;
    buf[..len].copy_from_slice(&tmp[i..]);
    core::str::from_utf8(&buf[..len]).unwrap_or("?")
}
