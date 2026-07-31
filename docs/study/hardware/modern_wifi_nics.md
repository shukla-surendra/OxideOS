# Modern Wi-Fi NICs vs. RTL8139 — Real-World Hardware Reference

**Source:** none in this codebase — this is a **reference-only** doc, not a
driver write-up. OxideOS has no Wi-Fi driver; its NICs are all wired
Ethernet (`drivers/net/rtl8139.rs`, `e1000.rs`, `pcnet.rs`). This doc exists
to answer a specific question: *how does the network chip in an actual
modern laptop compare to the RTL8139 this OS already drives* — same kind of
device talked to the same way, or something categorically different?

---

## What it is

Two things, compared:

1. **RTL8139** — the Fast Ethernet NIC OxideOS actually has a working
   driver for (see [`rtl8139.md`](rtl8139.md) in this folder). 100 Mbit/s,
   PCI, register-programmed, no firmware.
2. **The Wi-Fi/Bluetooth chip in a 2024–2026 laptop** — using this
   MacBook Pro (M4 Pro) as the measured example, plus the chips HP,
   Lenovo, Dell, and ASUS currently ship, researched via public
   documentation (sources at the bottom).

---

## Measured: what's actually in this MacBook Pro

Checked directly on this machine with `system_profiler` and `ioreg` —
not inferred:

| | |
|---|---|
| Model | Mac16,8 — MacBook Pro, Apple M4 Pro |
| Wi-Fi/Bluetooth chip | **Broadcom** — PCI vendor ID `0x14E4`, device ID `0x4388` |
| Driver class (macOS) | `AppleBCMWLANCore` / `AppleBCMWLANTimeSyncEngine` |
| PHY modes reported | 802.11 a/b/g/n/ac/ax |
| Built-in wired Ethernet | **none** — no onboard RJ45 chip on any MacBook Pro since ~2016 |

Vendor `0x14E4` (Broadcom) and the `AppleBCMWLANCore` driver class are
solid, directly-observed facts. The exact marketing part number ("BCM4388"
vs. a sibling in the family) is *not* pinned down here — two public PCI-ID
references disagreed on which device ID maps to that exact name, so this
doc doesn't assert one. What's certain: Apple has used a Broadcom
combo Wi-Fi/Bluetooth baseband, custom-packaged with Apple's own RF front
end, across every Apple Silicon Mac generation.

---

## What HP, Lenovo, Dell, and ASUS ship (2026)

The Wi-Fi 7 market has consolidated around four vendors. Critically, **the
CPU platform picks the Wi-Fi chip, not the OEM** — Intel doesn't sell Wi-Fi
silicon for AMD platforms, so an OEM's chip choice mostly just follows
whichever CPU is in that specific SKU:

| Chip | Vendor | Typical platform | Notes |
|---|---|---|---|
| **BE200** | Intel | Intel-CPU laptops (most Dell/HP business lines, many Lenovo ThinkPads) | Tri-band Wi-Fi 7; best driver maturity |
| **MT7925** | MediaTek | ASUS and Lenovo's common non-Intel default | On par with BE200, sometimes ahead on 6 GHz |
| **FastConnect 7800** | Qualcomm | AMD-CPU laptops (Ryzen-based Lenovo Legion, ASUS ROG, some HP Omen) | Best real-world throughput in independent benchmarks |
| **Killer BE1750 / Killer Wi-Fi 7** | MediaTek (Killer brand) | Gaming lines (some Legion, MSI, Predator) | "Killer" is a brand over MediaTek silicon, not a separate design |
| RTL8852-series | Realtek | Budget Windows laptops, Chromebooks | Wi-Fi 6/6E tier, not 7 — cost-driven pick |

Apple is the outlier here: it's the only major laptop vendor using a
**custom-packaged Broadcom** part instead of a merchant-silicon Intel/
MediaTek/Qualcomm module, because Apple designs the RF front end and
antenna system in-house around Broadcom's baseband, rather than buying a
complete M.2 module the way PC OEMs do.

---

## The comparison: is talking to one the same as talking to an RTL8139?

**Bus lineage: yes, directly related. Runtime protocol: no, categorically different.**

| | RTL8139 | Modern Wi-Fi (BCM4388-family, BE200, MT7925, FastConnect 7800) |
|---|---|---|
| **Bus** | PCI (parallel, shared, 33 MHz) | PCIe (serial, point-to-point) — PCI's direct successor |
| **Discovery** | Scan PCI config space, match `(vendor_id, device_id)` | Same *concept* — PCIe config space still has vendor/device IDs and BARs — but reached via PCIe's memory-mapped config mechanism (ECAM), not x86's legacy `0xCF8`/`0xCFC` port pair `pci.rs` uses |
| **Register model** | ~15 flat registers (`REG_RBSTART`, `REG_TCR`, `REG_ISR`, ...) — the driver programs the MAC directly | A small "doorbell"/mailbox register set — nowhere near enough to describe 802.11 behavior directly |
| **Firmware** | **None.** RTL8139 is a fixed-function ASIC; power it on and program the registers | **Required.** The host must upload a firmware blob to the chip at init (macOS: bundled in the OS; Linux: `iwlwifi-*.ucode`, `mt7925*.bin`, etc.) before the radio does anything at all |
| **Who implements 802.11** | N/A — Ethernet framing is simple enough the *host driver* does all of it | Mostly the **on-chip firmware** — association, WPA/RSN handshakes, channel scanning, rate control, and power management run on a small embedded CPU inside the Wi-Fi chip itself |
| **Host↔device communication style** | Direct register writes trigger direct hardware actions (write `REG_TSAD`+`REG_TSD` → DMA starts) | Structured **command/event messages** over a shared-memory ring — the driver sends something closer to an RPC call ("associate to this BSSID") and waits for a firmware-generated event, not a register poke |
| **DMA rings** | One RX ring, 4 TX slots — driver manages every byte | Many queues (per-QoS-priority data queues, separate management/command/event queues) — same *idea* (ring buffers, physical addresses, ownership bits) at far greater scale |
| **Interrupts** | One legacy IRQ line, a 16-bit ISR bitmask (`ISR_ROK`, `ISR_TOK`, ...) | MSI/MSI-X (many independent interrupt vectors, one bus transaction each instead of a shared line) |

### The verdict, plainly

RTL8139 is what's sometimes called a **"dumb NIC"**: every byte of the
Ethernet protocol is implemented by the driver you can read in
`rtl8139.rs` — the chip is a fast, fixed-function DMA engine and nothing
more. A modern Wi-Fi chip is a **coprocessor**: a small embedded system
in its own right, running vendor firmware that implements the actual
802.11 protocol stack, that the host driver talks to through a narrow,
firmware-defined command channel — closer in spirit to how OxideOS's
`ipc.rs` message queues let two *processes* exchange structured messages
than to how `rtl8139.rs` pokes hardware registers directly.

The PCI/PCIe bus-discovery layer is genuinely the same lineage — the
`(vendor_id, device_id)` matching pattern in `pci.rs`
(`pci::find_device(0x10EC, 0x8139)`) is architecturally the same idea
you'd use to find a Wi-Fi chip's PCIe function. Everything past that
point — the actual register/command protocol — is unrelated between the
two.

One more wrinkle specific to this MacBook: **Apple Silicon Macs don't
expose the legacy x86 PCI config mechanism at all.** `pci.rs`'s
`0xCF8`/`0xCFC` I/O-port scan is an x86-specific convention; Apple Silicon
enumerates its internal PCIe fabric through its own Device Tree—derived
mechanism instead. So even the "same bus lineage" claim above only holds
at the PCIe *specification* level — OxideOS's actual x86 PCI-scan code
couldn't find this Mac's Wi-Fi chip even if OxideOS ran on this hardware,
for reasons that have nothing to do with Wi-Fi specifically.

---

## Why OxideOS doesn't (and can't easily) support this class of device

1. **Firmware blobs are proprietary and chip-specific** — RTL8139 needed
   zero firmware to get working packet TX/RX; a Wi-Fi driver needs the
   *exact* signed firmware image the vendor ships for that chip revision,
   which OxideOS has no legal path to redistribute or load.
2. **The command/event protocol is undocumented and vendor-specific** —
   unlike RTL8139's register map (public, stable, documented on OSDev),
   Broadcom/Intel/MediaTek/Qualcomm's firmware command sets are internal
   APIs, reverse-engineered piecemeal by Linux driver maintainers over
   years (`brcmfmac`, `iwlwifi`, `mt76`, `ath12k`).
3. **802.11 itself is enormous** — even with firmware handling the hard
   radio-layer parts, the host side still needs 802.11 frame parsing,
   WPA2/3 key exchange, and a full supplicant — an order of magnitude more
   protocol surface than RTL8139's driver, which is ~600 lines total.

This is exactly why `docs/study/07_drivers.md`'s progression (port I/O →
interrupt-driven → PCI → MMIO virtio) stops short of Wi-Fi: every driver
in that series is a device you can fully understand and drive with
register-level control, in a few hundred lines, from a public datasheet.
Modern Wi-Fi silicon isn't in that category anymore — it's a second,
proprietary operating system running on the NIC that yours has to
negotiate with.

---

## Self-check questions

1. `pci.rs` finds RTL8139 via `pci::find_device(0x10EC, 0x8139)` — what two
   numbers would you need to find a specific Wi-Fi chip the same way, and
   where would you look them up?
2. Why does RTL8139 need zero firmware but a modern Wi-Fi chip needs a
   multi-hundred-KB firmware blob just to associate with a network? What
   changed between "Ethernet MAC" and "802.11 radio" that explains this?
3. RTL8139's `REG_ISR` is a flat bitmask read by polling or a single IRQ
   line. Why would a chip with a dozen internal queues (data ×4 priorities,
   command, event, management) want MSI-X instead of one shared IRQ?
4. This doc claims the PCI *bus-discovery* layer transfers to Wi-Fi chips
   but the *register protocol* doesn't. Pick one register from
   `rtl8139.md`'s table (e.g. `REG_TSAD`) and explain why there's no
   equivalent single register on a firmware-driven Wi-Fi chip.
5. Apple Silicon Macs skip x86's `0xCF8`/`0xCFC` PCI config ports entirely.
   If you were porting `pci.rs`'s device-discovery logic to run on Apple
   Silicon, what would have to change structurally, not just numerically?

---

## Sources

- [Qualcomm NCM865 Review: A Solid Wi-Fi 7 Upgrade](https://dongknows.com/qualcomm-ncm865-vs-intel-be200-wi-fi-7-upgrade/)
- [Laptop Wi-Fi Card Upgrade Guide 2026](https://computercompatibility.com/laptop-wifi-card-upgrade-guide/)
- [Upgrade Laptop to WiFi 7: M.2 2230 Card and Linux Driver Setup](https://botmonster.com/self-hosting/how-to-upgrade-your-laptop-to-wifi-7-2026/)
- [BCM4389 | Wi-Fi 6E and Bluetooth 5 Combo Chipset — Broadcom](https://www.broadcom.com/products/wireless/wireless-lan-bluetooth/bcm4389)
- [BCM4388 — device.report](https://device.report/broadcom/bcm4388)
- [MacBook Pro M4 Pro Teardown — iFixit](https://www.ifixit.com/News/106300/macbook-pro-m4-pro-teardown-new-model-same-repair-situation)
- Local measurement: `system_profiler SPAirPortDataType`, `ioreg -l` on this machine (2026-07-31)
