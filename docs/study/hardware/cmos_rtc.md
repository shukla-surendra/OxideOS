# CMOS / MC146818 — Real-Time Clock (RTC)

**Source:** `kernel/src/kernel/drivers/rtc.rs`

---

## What it is

The **MC146818** (or compatible) is a battery-backed chip that keeps the current
date and time even when the machine is powered off. It also contains 128 bytes of
**CMOS RAM** — small nonvolatile storage historically used by the BIOS to store
hardware configuration (boot order, memory settings, etc.).

The RTC is accessed through two I/O ports: you write an index to select which
internal register to read, then read/write the data through the data port.

---

## I/O Ports

| Port | Name | Purpose |
|------|------|---------|
| `0x70` | `CMOS_ADDR` | Write the index of the CMOS register you want to access |
| `0x71` | `CMOS_DATA` | Read or write the selected register |

**Caution:** Bit 7 of port `0x70` is the **NMI disable bit**. Writing `0x80 | register`
both selects the register *and* disables Non-Maskable Interrupts. OxideOS writes
the register index directly without setting bit 7, leaving NMIs enabled.

---

## RTC Registers

| Index | Name | Contents |
|-------|------|----------|
| `0x00` | `REG_SEC` | Seconds (0–59) |
| `0x02` | `REG_MIN` | Minutes (0–59) |
| `0x04` | `REG_HOUR` | Hours (0–23 or 1–12 with AM/PM bit) |
| `0x06` | `REG_WDAY` | Weekday (1=Sunday … 7=Saturday) |
| `0x07` | `REG_DAY` | Day of month (1–31) |
| `0x08` | `REG_MONTH` | Month (1–12) |
| `0x09` | `REG_YEAR` | Year within century (0–99) |
| `0x0A` | `REG_STATUS_A` | Bit 7 = update-in-progress flag |
| `0x0B` | `REG_STATUS_B` | Bit 2 = binary mode, Bit 1 = 24-hour mode |
| `0x32` | `REG_CENTURY` | Century (19 or 20 for 1900s/2000s) — not always present |

---

## Reading a register

```rust
// rtc.rs cmos_read() — line 38
fn cmos_read(reg: u8) -> u8 {
    unsafe {
        // 1. Write the register index to the address port
        asm!("out dx, al", in("dx") CMOS_ADDR, in("al") reg, ...);
        // 2. Read the value from the data port
        let v: u8;
        asm!("in al, dx", out("al") v, in("dx") CMOS_DATA, ...);
        v
    }
}
```

This pattern — write index, read data — is called an **indexed I/O** interface
and appears in many other chips (PIC ICW, VGA palette, etc.).

---

## The Update-In-Progress flag

The RTC hardware updates all its registers simultaneously once per second. If you
read during an update, you might get partially-written values (e.g., seconds just
rolled over but minutes haven't been incremented yet), causing subtle time errors.

**Status A, bit 7 = UIP (Update In Progress):**
- `1` = update is happening right now — don't read
- `0` = safe to read

OxideOS polls until clear before reading (`rtc.rs line 47–49`):
```rust
fn update_in_progress() -> bool {
    cmos_read(REG_STATUS_A) & 0x80 != 0
}
// then in read_time():
while update_in_progress() {}
```

---

## BCD vs Binary mode

The RTC can store values in two formats, selected by bit 2 of Status B:
- **BCD mode (bit 2 = 0):** each digit is stored as a 4-bit nibble.
  Example: hour `23` is stored as `0x23` (not `0x17`).
- **Binary mode (bit 2 = 1):** raw integer. Hour `23` = `0x17`.

Most real hardware boots in BCD mode. OxideOS handles both:
```rust
// rtc.rs line 51-53
fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd >> 4) * 10 + (bcd & 0x0F)
}

let binary_mode = status_b & 0x04 != 0;
let sec = if binary_mode { raw_sec } else { bcd_to_bin(raw_sec) };
```

`bcd_to_bin(0x23)`:
- Upper nibble: `0x23 >> 4 = 2`
- Lower nibble: `0x23 & 0x0F = 3`
- Result: `2 * 10 + 3 = 23` ✓

---

## 12-hour vs 24-hour mode

Status B, bit 1 selects the hour format:
- **12-hour mode (bit 1 = 0):** hours 1–12. Bit 7 of the hour register = PM.
- **24-hour mode (bit 1 = 1):** hours 0–23.

OxideOS handles both (`rtc.rs line 70–81`):
```rust
let (mut hour, pm) = if mode_24h {
    (bcd_to_bin(raw_hour), false)
} else {
    let pm_bit = raw_hour & 0x80 != 0;
    let h_raw  = raw_hour & 0x7F;  // mask off PM bit before converting
    (bcd_to_bin(h_raw), pm_bit)
};
if !mode_24h {
    if hour == 12 { hour = 0; }  // 12 AM = midnight = 0
    if pm { hour += 12; }        // 12 PM = noon, 1 PM = 13, etc.
}
```

---

## Timezone support

The hardware RTC **always stores UTC**. OxideOS applies a `TZ_OFFSET_MINUTES`
offset at display time (not stored in hardware):

```rust
// rtc.rs set_tz_offset / get_tz_offset — lines 30-36
static mut TZ_OFFSET_MINUTES: i32 = 0; // default = UTC

// apply_tz() — line 120
fn apply_tz(h24: u8, min: u8) -> (u8, u8, i32) {
    let total = h24 as i32 * 60 + min as i32 + TZ_OFFSET_MINUTES;
    // normalize to 0–1439 minutes (wraps at midnight)
    let total_norm = ((total % 1440) + 1440) % 1440;
    let day_delta = ...;  // -1 / 0 / +1 for midnight crossings
    ((total_norm / 60) as u8, (total_norm % 60) as u8, day_delta)
}
```

IST (India Standard Time) = UTC+5:30 → `set_tz_offset(5*60 + 30)` = `set_tz_offset(330)`.

---

## In OxideOS

| Function | What it returns |
|----------|----------------|
| `read_time()` | `(hour_24, minute, second)` in UTC |
| `read_date()` | `(weekday, day, month)` in UTC |
| `read_year()` | full 4-digit year (uses century register) |
| `format_time_hhmm(buf)` | `"HH:MM AM"` in local time |
| `format_time_ampm(buf)` | `"HH:MM:SS AM"` in local time |
| `format_date(buf)` | `"Ddd DD Mon"` e.g. `"Thu 01 May"` in local time |

The taskbar clock calls `format_time_hhmm()` and `format_date()` each frame.

---

## Common gotchas

**1. Not waiting for UIP clear.**
Reading during an update gives corrupted time values. Always spin on bit 7 of
Status A first.

**2. Forgetting BCD conversion.**
`cmos_read(REG_HOUR)` might return `0x13` (BCD) which is actually 1:00 PM (13h),
not hour 19. Always check Status B bit 2.

**3. Century register (0x32) not always present.**
Not all hardware has register 0x32. OxideOS reads it and validates the result —
if it's outside the range 19–22, it defaults to century 20 (years 2000–2099).

**4. The RTC drifts.**
The RTC crystal oscillates at 32,768 Hz but has ±20 ppm accuracy. Over a year
it can drift by ±10 minutes. Real systems sync to NTP to compensate.

---

## Self-check questions

1. Why must you wait for the UIP flag to clear before reading? What goes wrong without it?
2. Convert BCD value `0x59` to binary. What time field is this?
3. Why does OxideOS not store timezone in hardware? Where is it stored instead?
4. What is the `day_delta` return value from `apply_tz()` used for?
5. If `TZ_OFFSET_MINUTES = -300` (UTC-5, US Eastern), and the RTC reads 03:00 UTC,
   what local time will OxideOS display?
