//! CMOS Real-Time Clock (RTC) reader.
//!
//! Reads hours / minutes / seconds from the PC RTC via ports 0x70 / 0x71.
//! Handles both BCD and binary modes and both 12-hour and 24-hour formats.

use core::arch::asm;

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

const REG_SEC:    u8 = 0x00;
const REG_MIN:    u8 = 0x02;
const REG_HOUR:   u8 = 0x04;
const REG_STATUS_A: u8 = 0x0A;
const REG_STATUS_B: u8 = 0x0B;

fn cmos_read(reg: u8) -> u8 {
    unsafe {
        asm!("out dx, al", in("dx") CMOS_ADDR, in("al") reg, options(nomem, nostack));
        let v: u8;
        asm!("in al, dx", out("al") v, in("dx") CMOS_DATA, options(nomem, nostack));
        v
    }
}

fn update_in_progress() -> bool {
    cmos_read(REG_STATUS_A) & 0x80 != 0
}

fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd >> 4) * 10 + (bcd & 0x0F)
}

/// Returns `(hour_24, minute, second)` in 24-hour format (0–23).
pub fn read_time() -> (u8, u8, u8) {
    // Wait until the RTC is not in the middle of an update.
    while update_in_progress() {}

    let raw_sec  = cmos_read(REG_SEC);
    let raw_min  = cmos_read(REG_MIN);
    let raw_hour = cmos_read(REG_HOUR);
    let status_b = cmos_read(REG_STATUS_B);

    let binary_mode = status_b & 0x04 != 0;
    let mode_24h    = status_b & 0x02 != 0;

    let sec  = if binary_mode { raw_sec  } else { bcd_to_bin(raw_sec)  };
    let min  = if binary_mode { raw_min  } else { bcd_to_bin(raw_min)  };

    // In 12-hour mode the PM flag lives in bit 7 of the hour byte.
    let (mut hour, pm) = if mode_24h {
        (if binary_mode { raw_hour } else { bcd_to_bin(raw_hour) }, false)
    } else {
        let pm_bit = raw_hour & 0x80 != 0;
        let h_raw  = raw_hour & 0x7F;
        (if binary_mode { h_raw } else { bcd_to_bin(h_raw) }, pm_bit)
    };

    // Convert 12-hour to 24-hour if needed.
    if !mode_24h {
        if hour == 12 { hour = 0; }
        if pm { hour += 12; }
    }

    (hour, min, sec)
}

/// Fill `buf` (at least 11 bytes) with "HH:MM:SS AM" or "HH:MM:SS PM".
/// Returns the number of bytes written.
pub fn format_time_ampm(buf: &mut [u8]) -> usize {
    let (h24, min, sec) = read_time();

    let (h12, suffix) = match h24 {
        0        => (12u8, b"AM"),
        1..=11   => (h24,  b"AM"),
        12       => (12u8, b"PM"),
        _        => (h24 - 12, b"PM"),
    };

    if buf.len() < 11 { return 0; }
    buf[0] = b'0' + h12 / 10;
    buf[1] = b'0' + h12 % 10;
    buf[2] = b':';
    buf[3] = b'0' + min / 10;
    buf[4] = b'0' + min % 10;
    buf[5] = b':';
    buf[6] = b'0' + sec / 10;
    buf[7] = b'0' + sec % 10;
    buf[8] = b' ';
    buf[9]  = suffix[0];
    buf[10] = suffix[1];
    11
}
