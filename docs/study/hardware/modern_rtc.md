# Real-Time Clocks — CMOS/MC146818 vs. PMIC RTC / UEFI Time Services

**Companion to:** [cmos_rtc.md](cmos_rtc.md) (`kernel/src/kernel/drivers/rtc.rs`).
Reference-only.

---

## What it is

The job — keep approximate wall-clock time across a power-off — hasn't
changed. What's changed is *how little that job matters anymore*, because
essentially every device with the RTC also has a network connection, and
NTP (or an OS-level equivalent) has become the actual source of truth. The
hardware RTC's role has shrunk from "the clock" to "the fallback for the
first few seconds of boot, before anything else has synced."

---

## What replaced direct CMOS access

**x86 PCs, including brand-new ones:** the CMOS/MC146818-compatible
register block hasn't gone anywhere — it's part of the ACPI specification
itself (the "RTC" fixed-feature device), so every PCH (Platform Controller
Hub) still implements something register-compatible at ports `0x70`/`0x71`
for legacy/BIOS compatibility. What *has* changed is who talks to it
directly. Modern OSes don't bit-bang CMOS registers the way `rtc.rs` does
— they call **UEFI runtime services**, specifically `GetTime()` /
`SetTime()`, which abstract the actual RTC implementation away entirely.
Firmware is free to implement those calls however it wants underneath
(still often MC146818-compatible silicon, but the OS no longer needs to
know or care).

**ARM SoCs and mobile devices, including Apple Silicon:** the RTC
typically isn't part of the main SoC's register space at all — it lives
inside the **PMIC** (Power Management IC), a small, separate,
extremely-low-power chip whose entire job is staying alive on a tiny
battery/supercap reserve even when the rest of the system is fully
powered off. The main CPU talks to the PMIC's RTC over a slow serial bus
(I2C or SPI), not a memory-mapped or port-mapped register the way x86's
CMOS is — a deliberate trade-off, since the PMIC needs to run on
microwatts for potentially weeks between charges, and a fast parallel bus
would defeat that.

**Network time as the real source of truth:** on any internet-connected
device, NTP (or, on Apple platforms, a combination of NTP and an
Apple-signed secure time service tied to iCloud) corrects the clock within
seconds of getting online, every boot. The hardware RTC only matters for
the gap between power-on and that first sync — which is precisely the use
case OxideOS's own `cmos_rtc.md` gotchas section already flags: "the RTC
drifts... real systems sync to NTP to compensate." That line is describing
exactly this shift, just from the angle of drift correction rather than
absolute source-of-truth.

---

## Side by side

| | CMOS/MC146818 (direct) | UEFI runtime services | PMIC RTC (ARM/mobile) |
|---|---|---|---|
| Access | Indexed port I/O (`0x70`/`0x71`) | Firmware call (`GetTime`/`SetTime`) | Serial bus (I2C/SPI) to a separate chip |
| Who talks to it | The OS directly (what `rtc.rs` does) | The OS, via an abstraction firmware implements | The SoC, via a driver for the PMIC's I2C/SPI interface |
| Power domain | On the motherboard, small coin-cell battery | Same underlying hardware, hidden behind the call | A dedicated ultra-low-power chip, separate battery/supercap budget |
| Primary role today | Legacy compatibility | The "correct" way for a modern x86 OS to read/set time | Same fallback role, mobile-power-budget-first design |

---

## Why OxideOS talks to CMOS directly

Same throughline as this whole doc series: no firmware abstraction layer
to implement first (UEFI runtime services require having already parsed
the UEFI system table Limine hands off), fixed well-known ports, and a
genuinely simple protocol (`cmos_rtc.md`'s "indexed I/O" pattern — write a
register index, read the data port). The BCD/24-hour-mode/UIP handling
`rtc.rs` already does is real complexity that UEFI's `GetTime()` call
would hide entirely — calling `GetTime()` on real firmware gets you an
already-normalized, already-decoded time struct, no BCD conversion
required. OxideOS choosing the raw path is, in effect, choosing to
implement in software exactly what UEFI firmware implements on your
behalf on every real modern machine.

---

## Self-check questions

1. Why does the ACPI spec require every PC-compatible motherboard to keep
   an MC146818-compatible RTC block, decades after most software stopped
   talking to it directly?
2. What has to be true about the boot sequence for OxideOS to call CMOS
   registers directly the way it does, that wouldn't be true if it instead
   wanted to call UEFI's `GetTime()`?
3. Why does a PMIC's RTC use a serial bus (I2C/SPI) instead of a
   memory-mapped register the CPU can read directly, the way x86's CMOS
   works?
4. `rtc.rs` stores a `TZ_OFFSET_MINUTES` in software rather than hardware
   (`cmos_rtc.md`'s own notes: "the hardware RTC always stores UTC"). Why
   would a PMIC-based mobile RTC make the exact same design choice?
5. If NTP sync happens within seconds of every boot on a connected device,
   what's the actual remaining value of the hardware RTC being accurate at
   all — what specific scenario does it still matter for?

---

## Sources

- `docs/study/hardware/cmos_rtc.md` (this codebase)
- ACPI Specification (RTC fixed-feature device), UEFI Specification (Time Services: `GetTime`/`SetTime`) — standard references
