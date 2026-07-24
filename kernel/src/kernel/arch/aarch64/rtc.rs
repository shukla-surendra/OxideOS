//! PL031 real-time clock (QEMU virt @ phys 0x0901_0000).
//!
//! The PL031 reports seconds since the Unix epoch in its DR register; the
//! calendar math below converts that to the same `(h, m, s)` / `(wday, day,
//! month)` / year API the x86 CMOS driver exposes, so the taskbar clock,
//! calendar, and quick-settings panels work unchanged.

const PL031_PHYS: u64 = 0x0901_0000;

/// UTC offset in minutes. Negative for west, positive for east.
static mut TZ_OFFSET_MINUTES: i32 = 0;

pub fn get_tz_offset() -> i32 {
    unsafe { TZ_OFFSET_MINUTES }
}

pub fn set_tz_offset(minutes: i32) {
    unsafe { TZ_OFFSET_MINUTES = minutes; }
}

/// Seconds since the Unix epoch, straight from the PL031 data register.
fn epoch_seconds() -> u64 {
    let hhdm = crate::HHDM_REQUEST
        .get_response()
        .map(|r| r.offset())
        .unwrap_or(0);
    unsafe { ((hhdm + PL031_PHYS) as *const u32).read_volatile() as u64 }
}

/// Days since epoch → (year, month 1-12, day 1-31).  Howard Hinnant's
/// civil-from-days algorithm.
fn civil_from_days(z: i64) -> (i64, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Returns `(hour_24, minute, second)` as **raw UTC** from the hardware.
pub fn read_time() -> (u8, u8, u8) {
    let secs = epoch_seconds();
    (
        ((secs / 3600) % 24) as u8,
        ((secs / 60) % 60) as u8,
        (secs % 60) as u8,
    )
}

/// Returns the full 4-digit year (e.g. 2026).
pub fn read_year() -> u32 {
    let days = (epoch_seconds() / 86_400) as i64;
    civil_from_days(days).0 as u32
}

/// Returns `(weekday, day, month)` as **raw UTC**.  weekday: 1=Sun … 7=Sat.
pub fn read_date() -> (u8, u8, u8) {
    let days = (epoch_seconds() / 86_400) as i64;
    let (_, month, day) = civil_from_days(days);
    // 1970-01-01 was a Thursday (weekday 5 in the 1=Sun scheme).
    let wday = ((days + 4).rem_euclid(7) + 1) as u8;
    (wday, day, month)
}

/// Apply the current TZ offset to a UTC (h24, min) pair.
/// Returns `(local_h24, local_min, day_delta)` where `day_delta` is -1/0/+1.
fn apply_tz(h24: u8, min: u8) -> (u8, u8, i32) {
    let tz = get_tz_offset();
    let total = h24 as i32 * 60 + min as i32 + tz;
    let total_norm = ((total % 1440) + 1440) % 1440;
    let day_delta = if total < 0 { -1 } else if total >= 1440 { 1 } else { 0 };
    ((total_norm / 60) as u8, (total_norm % 60) as u8, day_delta)
}

/// Fill `buf` (≥ 8 bytes) with local "HH:MM AM" / "HH:MM PM". Returns bytes written.
pub fn format_time_hhmm(buf: &mut [u8]) -> usize {
    let (h24_utc, min_utc, _) = read_time();
    let (h24, min, _) = apply_tz(h24_utc, min_utc);

    let (h12, suffix) = match h24 {
        0 => (12u8, b"AM"),
        1..=11 => (h24, b"AM"),
        12 => (12u8, b"PM"),
        _ => (h24 - 12, b"PM"),
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
pub fn format_date(buf: &mut [u8]) -> usize {
    let (wday_utc, day_utc, month) = read_date();
    let (h24_utc, min_utc, _) = read_time();
    let (_, _, day_delta) = apply_tz(h24_utc, min_utc);

    let wday = ((wday_utc as i32 - 1 + day_delta + 7) % 7 + 1) as u8;
    let day = (day_utc as i32 + day_delta).clamp(1, 31) as u8;

    const DAYS: [&[u8]; 8] = [b"???", b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
    const MONTHS: [&[u8]; 13] = [
        b"???", b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun",
        b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec",
    ];

    let dname = DAYS[wday.min(7) as usize];
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

/// Fill `buf` (≥ 11 bytes) with local "HH:MM:SS AM" / "HH:MM:SS PM".
/// Returns bytes written.
pub fn format_time_ampm(buf: &mut [u8]) -> usize {
    let (h24_utc, min_utc, sec) = read_time();
    let (h24, min, _) = apply_tz(h24_utc, min_utc);

    let (h12, suffix) = match h24 {
        0 => (12u8, b"AM"),
        1..=11 => (h24, b"AM"),
        12 => (12u8, b"PM"),
        _ => (h24 - 12, b"PM"),
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
    buf[9] = suffix[0];
    buf[10] = suffix[1];
    11
}
