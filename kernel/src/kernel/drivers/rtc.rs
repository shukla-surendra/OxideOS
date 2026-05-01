//! CMOS Real-Time Clock (RTC) reader.
//!
//! Reads hours / minutes / seconds from the PC RTC via ports 0x70 / 0x71.
//! Handles both BCD and binary modes and both 12-hour and 24-hour formats.
//!
//! **The hardware RTC always stores UTC.**  A kernel-wide `TZ_OFFSET_MINUTES`
//! (default 0 = UTC) is applied by all `format_*` functions so every time
//! display shows local time.  Set it via `set_tz_offset()`.

use core::arch::asm;

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

const REG_SEC:      u8 = 0x00;
const REG_MIN:      u8 = 0x02;
const REG_HOUR:     u8 = 0x04;
const REG_WDAY:     u8 = 0x06; // 1=Sun … 7=Sat
const REG_DAY:      u8 = 0x07;
const REG_MONTH:    u8 = 0x08;
const REG_YEAR:     u8 = 0x09; // 0-99 (two-digit)
const REG_CENTURY:  u8 = 0x32; // BCD century (20 = year 20xx)
const REG_STATUS_A: u8 = 0x0A;
const REG_STATUS_B: u8 = 0x0B;

/// UTC offset in minutes. Negative for west, positive for east.
/// Set via `set_tz_offset`; read via `get_tz_offset`.
static mut TZ_OFFSET_MINUTES: i32 = 0;

pub fn get_tz_offset() -> i32 {
    unsafe { TZ_OFFSET_MINUTES }
}

pub fn set_tz_offset(minutes: i32) {
    unsafe { TZ_OFFSET_MINUTES = minutes; }
}

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

/// Returns `(hour_24, minute, second)` as **raw UTC** from the hardware.
pub fn read_time() -> (u8, u8, u8) {
    while update_in_progress() {}

    let raw_sec  = cmos_read(REG_SEC);
    let raw_min  = cmos_read(REG_MIN);
    let raw_hour = cmos_read(REG_HOUR);
    let status_b = cmos_read(REG_STATUS_B);

    let binary_mode = status_b & 0x04 != 0;
    let mode_24h    = status_b & 0x02 != 0;

    let sec  = if binary_mode { raw_sec  } else { bcd_to_bin(raw_sec)  };
    let min  = if binary_mode { raw_min  } else { bcd_to_bin(raw_min)  };

    let (mut hour, pm) = if mode_24h {
        (if binary_mode { raw_hour } else { bcd_to_bin(raw_hour) }, false)
    } else {
        let pm_bit = raw_hour & 0x80 != 0;
        let h_raw  = raw_hour & 0x7F;
        (if binary_mode { h_raw } else { bcd_to_bin(h_raw) }, pm_bit)
    };

    if !mode_24h {
        if hour == 12 { hour = 0; }
        if pm { hour += 12; }
    }

    (hour, min, sec)
}

/// Returns the full 4-digit year from the CMOS RTC (e.g. 2026).
/// Uses the century register (0x32) if present; falls back to 21st century.
pub fn read_year() -> u32 {
    while update_in_progress() {}
    let raw_year = cmos_read(REG_YEAR);
    let raw_cent = cmos_read(REG_CENTURY);
    let status_b = cmos_read(REG_STATUS_B);
    let binary   = status_b & 0x04 != 0;

    let year2 = if binary { raw_year } else { bcd_to_bin(raw_year) } as u32;
    let cent  = if binary { raw_cent } else { bcd_to_bin(raw_cent) } as u32;
    let century = if cent >= 19 && cent <= 22 { cent } else { 20 };
    century * 100 + year2
}

/// Returns `(weekday, day, month)` as **raw UTC** from the hardware.
/// weekday: 1=Sun … 7=Sat.
pub fn read_date() -> (u8, u8, u8) {
    while update_in_progress() {}

    let raw_wday  = cmos_read(REG_WDAY);
    let raw_day   = cmos_read(REG_DAY);
    let raw_month = cmos_read(REG_MONTH);
    let status_b  = cmos_read(REG_STATUS_B);
    let binary    = status_b & 0x04 != 0;

    let wday  = if binary { raw_wday  } else { bcd_to_bin(raw_wday)  };
    let day   = if binary { raw_day   } else { bcd_to_bin(raw_day)   };
    let month = if binary { raw_month } else { bcd_to_bin(raw_month) };
    (wday, day, month)
}

/// Apply the current TZ offset to a UTC (h24, min) pair.
/// Returns `(local_h24, local_min, day_delta)` where `day_delta` is -1/0/+1.
fn apply_tz(h24: u8, min: u8) -> (u8, u8, i32) {
    let tz = get_tz_offset();
    let total = h24 as i32 * 60 + min as i32 + tz;
    let total_norm = ((total % 1440) + 1440) % 1440;
    let day_delta  = if total < 0 { -1 } else if total >= 1440 { 1 } else { 0 };
    ((total_norm / 60) as u8, (total_norm % 60) as u8, day_delta)
}

/// Fill `buf` (≥ 8 bytes) with local "HH:MM AM" / "HH:MM PM". Returns bytes written.
pub fn format_time_hhmm(buf: &mut [u8]) -> usize {
    let (h24_utc, min_utc, _) = read_time();
    let (h24, min, _) = apply_tz(h24_utc, min_utc);

    let (h12, suffix) = match h24 {
        0        => (12u8, b"AM"),
        1..=11   => (h24,  b"AM"),
        12       => (12u8, b"PM"),
        _        => (h24 - 12, b"PM"),
    };

    if buf.len() < 8 { return 0; }
    buf[0] = b'0' + h12 / 10;
    buf[1] = b'0' + h12 % 10;
    buf[2] = b':';
    buf[3] = b'0' + min / 10;
    buf[4] = b'0' + min % 10;
    buf[5] = b' ';
    buf[6] = suffix[0];
    buf[7] = suffix[1];
    8
}

/// Fill `buf` (≥ 10 bytes) with local "Ddd DD Mon" e.g. "Thu 01 May". Returns bytes written.
/// Day-of-month adjustment for midnight crossings is approximate at month boundaries.
pub fn format_date(buf: &mut [u8]) -> usize {
    let (wday_utc, day_utc, month) = read_date();
    let (h24_utc, min_utc, _)      = read_time();
    let (_, _, day_delta)          = apply_tz(h24_utc, min_utc);

    // Adjust weekday (wraps correctly within 1-7)
    let wday = ((wday_utc as i32 - 1 + day_delta + 7) % 7 + 1) as u8;
    // Adjust day (clamped; month-boundary crossings show ±1 day without month wrap)
    let day  = (day_utc as i32 + day_delta).clamp(1, 31) as u8;

    const DAYS:   [&[u8]; 8] = [b"???", b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
    const MONTHS: [&[u8]; 13] = [
        b"???", b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun",
        b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec",
    ];

    let dname = DAYS  [wday .min(7)  as usize];
    let mname = MONTHS[month.min(12) as usize];

    if buf.len() < 10 { return 0; }
    buf[0..3].copy_from_slice(dname);
    buf[3] = b' ';
    buf[4] = b'0' + day / 10;
    buf[5] = b'0' + day % 10;
    buf[6] = b' ';
    buf[7..10].copy_from_slice(mname);
    10
}

/// Fill `buf` (≥ 11 bytes) with local "HH:MM:SS AM" or "HH:MM:SS PM".
/// Returns bytes written.
pub fn format_time_ampm(buf: &mut [u8]) -> usize {
    let (h24_utc, min_utc, sec) = read_time();
    let (h24, min, _) = apply_tz(h24_utc, min_utc);

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
