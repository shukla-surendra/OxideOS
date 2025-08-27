use core::arch::asm;

/// Read a byte from an I/O port.
#[inline]
pub fn in8(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    val
}

/// Write a byte to an I/O port (not used yet but handy).
#[allow(dead_code)] // #[inline]
pub fn out8(port: u16, val: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}
