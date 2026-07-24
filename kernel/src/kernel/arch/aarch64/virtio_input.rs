//! virtio-input driver over virtio-mmio — polled, no GIC required.
//!
//! QEMU's `virt` machine exposes 32 virtio-mmio transport slots at
//! 0x0a00_0000 (stride 0x200); `-device virtio-keyboard-device` /
//! `-device virtio-mouse-device` land in them.  We probe every slot for an
//! input device (device ID 18), give each one an event queue of 8-byte
//! `virtio_input_event` buffers, and drain the used ring from the GUI loop
//! each frame — the same polling model the rest of the aarch64 port uses
//! until the GIC lands.
//!
//! Events use evdev semantics.  Relative motion and button events feed the
//! shared mouse cursor state (`kernel::interrupts`); key events are
//! translated from evdev keycodes to scancode-set-1 bytes and injected into
//! the shared `drivers/keyboard` decoder, so the whole x86 key-dispatch
//! pipeline (stdin, GUI callbacks, modifiers) works unchanged.
//!
//! Registers are reached through the Limine HHDM (base revision 2 maps MMIO;
//! see docs/arm/02-boot-and-exceptions.md).  Ring/buffer memory comes from
//! the kernel heap, which lives in the HHDM, so `phys = virt - hhdm_offset`.

use alloc::alloc::{alloc_zeroed, Layout};
use alloc::vec::Vec;
use core::sync::atomic::{fence, Ordering};

use crate::kernel::serial::SERIAL_PORT;

/// Log every event (type/code/value) and dispatch stages to serial.
const INPUT_EVENT_DEBUG_LOGGING: bool = false;

// ── QEMU virt virtio-mmio window ─────────────────────────────────────────────

const MMIO_BASE: u64 = 0x0a00_0000;
const MMIO_STRIDE: u64 = 0x200;
const MMIO_SLOTS: u64 = 32;

// ── virtio-mmio registers (offsets from a slot base) ─────────────────────────

const REG_MAGIC: u64 = 0x000; // "virt" = 0x74726976
const REG_VERSION: u64 = 0x004; // 1 = legacy, 2 = modern
const REG_DEVICE_ID: u64 = 0x008; // 18 = input
const REG_DRV_FEATURES: u64 = 0x020;
const REG_DRV_FEATURES_SEL: u64 = 0x024;
const REG_GUEST_PAGE_SIZE: u64 = 0x028; // legacy only
const REG_QUEUE_SEL: u64 = 0x030;
const REG_QUEUE_NUM_MAX: u64 = 0x034;
const REG_QUEUE_NUM: u64 = 0x038;
const REG_QUEUE_ALIGN: u64 = 0x03c; // legacy only
const REG_QUEUE_PFN: u64 = 0x040; // legacy only
const REG_QUEUE_READY: u64 = 0x044; // modern only
const REG_QUEUE_NOTIFY: u64 = 0x050;
const REG_INT_STATUS: u64 = 0x060;
const REG_INT_ACK: u64 = 0x064;
const REG_STATUS: u64 = 0x070;
const REG_QUEUE_DESC_LOW: u64 = 0x080; // modern only
const REG_QUEUE_DESC_HIGH: u64 = 0x084;
const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;
const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;
const REG_CONFIG: u64 = 0x100;

const MAGIC_VIRT: u32 = 0x7472_6976;
const DEVICE_ID_INPUT: u32 = 18;

// Device status bits.
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;

// virtio-input config space selectors.
const CFG_ID_NAME: u8 = 0x01;

// Virtqueue constants.
const QUEUE_EVENTQ: u32 = 0;
const QUEUE_SIZE_CAP: u16 = 64;
const PAGE_SIZE: u64 = 4096;
const DESC_F_WRITE: u16 = 2;
const USED_F_NO_NOTIFY: u16 = 1;

// evdev event types / codes (virtio-input reuses them verbatim).
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;

/// Split virtqueue descriptor (virtio spec 2.7.5).
#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// One initialised virtio-input device with its event queue.
/// (The descriptor table is filled once in `init_device` and never touched
/// again — every descriptor permanently maps its same-index buffer.)
struct InputDev {
    regs: u64, // virtual base of the MMIO register block
    qsize: u16,
    avail: *mut u8, // flags u16, idx u16, ring[qsize] u16
    used: *mut u8,  // flags u16, idx u16, ring[qsize] {id u32, len u32}
    buf: *mut u8,   // qsize contiguous 8-byte event buffers
    avail_idx: u16, // shadow of avail->idx (driver-owned)
    last_used: u16, // next used->idx we have not consumed yet
}

static mut DEVICES: Vec<InputDev> = Vec::new();

// ── MMIO helpers ─────────────────────────────────────────────────────────────

unsafe fn reg_read(base: u64, off: u64) -> u32 {
    unsafe { ((base + off) as *const u32).read_volatile() }
}

unsafe fn reg_write(base: u64, off: u64, val: u32) {
    unsafe { ((base + off) as *mut u32).write_volatile(val) }
}

unsafe fn read_u16(p: *const u8) -> u16 {
    unsafe { (p as *const u16).read_volatile() }
}

unsafe fn write_u16(p: *mut u8, v: u16) {
    unsafe { (p as *mut u16).write_volatile(v) }
}

// ── Initialisation ───────────────────────────────────────────────────────────

/// Probe the virtio-mmio window and bring up every input device found.
/// Requires the heap (ring allocations) and the Limine HHDM.
pub unsafe fn init() {
    let Some(hhdm) = crate::HHDM_REQUEST.get_response().map(|r| r.offset()) else {
        unsafe { SERIAL_PORT.write_str("virtio-input: no HHDM response, skipping\n"); }
        return;
    };

    let mut found = 0u32;
    for slot in 0..MMIO_SLOTS {
        let base = hhdm + MMIO_BASE + slot * MMIO_STRIDE;
        unsafe {
            if reg_read(base, REG_MAGIC) != MAGIC_VIRT {
                continue;
            }
            if reg_read(base, REG_DEVICE_ID) != DEVICE_ID_INPUT {
                continue;
            }
            let version = reg_read(base, REG_VERSION);
            if version != 1 && version != 2 {
                continue;
            }
            if let Some(dev) = init_device(base, version, hhdm) {
                log_device_name(base);
                (*core::ptr::addr_of_mut!(DEVICES)).push(dev);
                found += 1;
            }
        }
    }

    unsafe {
        SERIAL_PORT.write_str("virtio-input: ");
        SERIAL_PORT.write_decimal(found);
        SERIAL_PORT.write_str(" input device(s) ready\n");
    }
}

/// Bring one device from reset to DRIVER_OK with a populated event queue.
unsafe fn init_device(base: u64, version: u32, hhdm: u64) -> Option<InputDev> {
    unsafe {
        let legacy = version == 1;

        // Status dance: reset → ACKNOWLEDGE → DRIVER.
        reg_write(base, REG_STATUS, 0);
        reg_write(base, REG_STATUS, STATUS_ACKNOWLEDGE);
        reg_write(base, REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // Feature negotiation.  We need none of the optional features; modern
        // devices additionally require VIRTIO_F_VERSION_1 (bit 32).
        reg_write(base, REG_DRV_FEATURES_SEL, 1);
        reg_write(base, REG_DRV_FEATURES, if legacy { 0 } else { 1 });
        reg_write(base, REG_DRV_FEATURES_SEL, 0);
        reg_write(base, REG_DRV_FEATURES, 0);

        let mut status = STATUS_ACKNOWLEDGE | STATUS_DRIVER;
        if !legacy {
            status |= STATUS_FEATURES_OK;
            reg_write(base, REG_STATUS, status);
            if reg_read(base, REG_STATUS) & STATUS_FEATURES_OK == 0 {
                SERIAL_PORT.write_str("virtio-input: device rejected features\n");
                reg_write(base, REG_STATUS, STATUS_FAILED);
                return None;
            }
        }

        // Event queue geometry.
        reg_write(base, REG_QUEUE_SEL, QUEUE_EVENTQ);
        let max = reg_read(base, REG_QUEUE_NUM_MAX);
        if max == 0 {
            reg_write(base, REG_STATUS, STATUS_FAILED);
            return None;
        }
        let qsize = (max as u16).min(QUEUE_SIZE_CAP);

        // Ring memory in the legacy contiguous layout (descriptor table,
        // avail ring, page-aligned used ring).  The modern interface takes
        // the three addresses separately, so the same block works for both.
        let desc_bytes = 16 * qsize as u64;
        let avail_bytes = 6 + 2 * qsize as u64;
        let used_off = (desc_bytes + avail_bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let used_bytes = 6 + 8 * qsize as u64;
        let ring_total = (used_off + used_bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        let ring = alloc_zeroed(Layout::from_size_align(ring_total as usize, PAGE_SIZE as usize).ok()?);
        if ring.is_null() {
            return None;
        }
        let ring_phys = ring as u64 - hhdm;

        // Event buffers: qsize × 8-byte virtio_input_event.
        let buf = alloc_zeroed(Layout::from_size_align(8 * qsize as usize, 8).ok()?);
        if buf.is_null() {
            return None;
        }
        let buf_phys = buf as u64 - hhdm;

        let desc = ring as *mut VirtqDesc;
        let avail = ring.add(desc_bytes as usize);
        let used = ring.add(used_off as usize);

        // Every descriptor points at its buffer, device-writable, and sits in
        // the avail ring from the start.
        for i in 0..qsize {
            desc.add(i as usize).write(VirtqDesc {
                addr: buf_phys + 8 * i as u64,
                len: 8,
                flags: DESC_F_WRITE,
                next: 0,
            });
            write_u16(avail.add(4 + 2 * i as usize), i);
        }
        write_u16(avail, 0); // flags
        fence(Ordering::Release);
        write_u16(avail.add(2), qsize); // idx: all buffers published

        reg_write(base, REG_QUEUE_NUM, qsize as u32);
        if legacy {
            reg_write(base, REG_GUEST_PAGE_SIZE, PAGE_SIZE as u32);
            reg_write(base, REG_QUEUE_ALIGN, PAGE_SIZE as u32);
            reg_write(base, REG_QUEUE_PFN, (ring_phys / PAGE_SIZE) as u32);
        } else {
            let avail_phys = ring_phys + desc_bytes;
            let used_phys = ring_phys + used_off;
            reg_write(base, REG_QUEUE_DESC_LOW, ring_phys as u32);
            reg_write(base, REG_QUEUE_DESC_HIGH, (ring_phys >> 32) as u32);
            reg_write(base, REG_QUEUE_DRIVER_LOW, avail_phys as u32);
            reg_write(base, REG_QUEUE_DRIVER_HIGH, (avail_phys >> 32) as u32);
            reg_write(base, REG_QUEUE_DEVICE_LOW, used_phys as u32);
            reg_write(base, REG_QUEUE_DEVICE_HIGH, (used_phys >> 32) as u32);
            reg_write(base, REG_QUEUE_READY, 1);
        }

        reg_write(base, REG_STATUS, status | STATUS_DRIVER_OK);
        reg_write(base, REG_QUEUE_NOTIFY, QUEUE_EVENTQ);

        Some(InputDev {
            regs: base,
            qsize,
            avail,
            used,
            buf,
            avail_idx: qsize,
            last_used: 0,
        })
    }
}

/// Log the device's self-reported name (config selector VIRTIO_INPUT_CFG_ID_NAME).
unsafe fn log_device_name(base: u64) {
    unsafe {
        let cfg = (base + REG_CONFIG) as *mut u8;
        cfg.write_volatile(CFG_ID_NAME); // select
        cfg.add(1).write_volatile(0); // subsel
        let size = cfg.add(2).read_volatile();

        SERIAL_PORT.write_str("virtio-input: found \"");
        for i in 0..size.min(64) {
            let b = cfg.add(8 + i as usize).read_volatile();
            if b == 0 {
                break;
            }
            if b.is_ascii_graphic() || b == b' ' {
                SERIAL_PORT.write_byte(b);
            }
        }
        SERIAL_PORT.write_str("\"\n");
    }
}

// ── Polling + event dispatch ─────────────────────────────────────────────────

/// Drain every device's used ring and dispatch the events.  Called from the
/// GUI loop each frame (via `keyboard::poll` and `interrupts::poll_mouse_data`
/// — draining twice per frame is harmless).  Returns true if any event arrived.
pub unsafe fn poll() -> bool {
    let mut any = false;
    unsafe {
        for dev in (*core::ptr::addr_of_mut!(DEVICES)).iter_mut() {
            // We poll, but ack interrupt status anyway so state stays clean
            // for the future GIC-driven version.
            let isr = reg_read(dev.regs, REG_INT_STATUS);
            if isr != 0 {
                reg_write(dev.regs, REG_INT_ACK, isr);
            }

            let mut recycled = false;
            loop {
                fence(Ordering::Acquire);
                let used_idx = read_u16(dev.used.add(2));
                if used_idx == dev.last_used {
                    break;
                }

                let slot = (dev.last_used % dev.qsize) as usize;
                let id = (dev.used.add(4 + 8 * slot) as *const u32).read_volatile() as u16;
                if id < dev.qsize {
                    let ev = dev.buf.add(8 * id as usize);
                    let etype = read_u16(ev);
                    let code = read_u16(ev.add(2));
                    let value = (ev.add(4) as *const u32).read_volatile();
                    if INPUT_EVENT_DEBUG_LOGGING {
                        SERIAL_PORT.write_str("[vinput t=");
                        SERIAL_PORT.write_hex(etype as u32);
                        SERIAL_PORT.write_str(" c=");
                        SERIAL_PORT.write_hex(code as u32);
                        SERIAL_PORT.write_str(" v=");
                        SERIAL_PORT.write_hex(value);
                        SERIAL_PORT.write_str("]");
                    }
                    handle_event(etype, code, value);
                    if INPUT_EVENT_DEBUG_LOGGING {
                        SERIAL_PORT.write_str("[done]\n");
                    }

                    // Hand the buffer back to the device.
                    let avail_slot = (dev.avail_idx % dev.qsize) as usize;
                    write_u16(dev.avail.add(4 + 2 * avail_slot), id);
                    fence(Ordering::Release);
                    dev.avail_idx = dev.avail_idx.wrapping_add(1);
                    write_u16(dev.avail.add(2), dev.avail_idx);
                    recycled = true;
                }
                dev.last_used = dev.last_used.wrapping_add(1);
                any = true;
            }

            if recycled && read_u16(dev.used) & USED_F_NO_NOTIFY == 0 {
                reg_write(dev.regs, REG_QUEUE_NOTIFY, QUEUE_EVENTQ);
            }
        }
    }
    any
}

/// Route one evdev event into the shared mouse/keyboard state.
unsafe fn handle_event(etype: u16, code: u16, value: u32) {
    use crate::kernel::interrupts::{MOUSE_CONTROLLER, MOUSE_CURSOR, SCREEN_DIMENSIONS};

    match etype {
        EV_KEY => {
            let pressed = value != 0; // 1 = press, 2 = autorepeat
            match code {
                BTN_LEFT | BTN_RIGHT | BTN_MIDDLE => unsafe {
                    if let Some(mouse) = (*core::ptr::addr_of_mut!(MOUSE_CONTROLLER)).as_mut() {
                        match code {
                            BTN_LEFT => mouse.left_button = pressed,
                            BTN_RIGHT => mouse.right_button = pressed,
                            _ => mouse.middle_button = pressed,
                        }
                    }
                },
                _ => unsafe { feed_key(code, pressed) },
            }
        }
        EV_REL => unsafe {
            let (w, h) = *core::ptr::addr_of!(SCREEN_DIMENSIONS);
            if let Some(cursor) = (*core::ptr::addr_of_mut!(MOUSE_CURSOR)).as_mut() {
                let d = (value as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                match code {
                    REL_X => cursor.update(d, 0, w, h),
                    // evdev Y grows downward; MouseCursor::update expects the
                    // PS/2 convention (positive = up), so flip the sign.
                    REL_Y => cursor.update(0, -d, w, h),
                    _ => {} // wheel etc. — nothing consumes these yet
                }
            }
        },
        _ => {} // EV_SYN and friends need no action in this pipeline
    }
}

/// Translate an evdev keycode into scancode-set-1 bytes and feed the shared
/// keyboard decoder.  Autorepeat re-sends the make code, matching real PS/2.
unsafe fn feed_key(code: u16, pressed: bool) {
    let Some((extended, sc)) = evdev_to_set1(code) else {
        return;
    };
    unsafe {
        if extended {
            crate::kernel::keyboard::process_scancode(0xE0);
        }
        crate::kernel::keyboard::process_scancode(if pressed { sc } else { sc | 0x80 });
    }
}

/// evdev keycode → (E0-prefixed?, set-1 make code).
///
/// Codes 1–88 (the original XT block: letters, digits, F1–F12, keypad,
/// modifiers) are numerically identical in both encodings.  The navigation /
/// right-hand-modifier cluster maps to E0-prefixed codes.
fn evdev_to_set1(code: u16) -> Option<(bool, u8)> {
    match code {
        1..=88 => Some((false, code as u8)),
        96 => Some((true, 0x1C)),  // keypad enter
        97 => Some((true, 0x1D)),  // right ctrl
        98 => Some((true, 0x35)),  // keypad slash
        100 => Some((true, 0x38)), // right alt
        102 => Some((true, 0x47)), // home
        103 => Some((true, 0x48)), // up
        104 => Some((true, 0x49)), // page up
        105 => Some((true, 0x4B)), // left
        106 => Some((true, 0x4D)), // right
        107 => Some((true, 0x4F)), // end
        108 => Some((true, 0x50)), // down
        109 => Some((true, 0x51)), // page down
        110 => Some((true, 0x52)), // insert
        111 => Some((true, 0x53)), // delete
        125 => Some((true, 0x5B)), // left meta
        126 => Some((true, 0x5C)), // right meta
        _ => None,
    }
}
