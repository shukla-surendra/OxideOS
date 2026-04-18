//! Global environment variable store for OxideOS.
//!
//! Stores up to 32 key=value pairs, accessible via the Getenv (79) and
//! Setenv (80) syscalls.

const MAX_VARS: usize = 32;
const MAX_KEY:  usize = 32;
const MAX_VAL:  usize = 256;

#[repr(C)]
struct EnvEntry {
    key:     [u8; MAX_KEY],
    key_len: u8,
    val:     [u8; MAX_VAL],
    val_len: u16,
    used:    bool,
}

impl EnvEntry {
    const fn empty() -> Self {
        Self { key: [0u8; MAX_KEY], key_len: 0, val: [0u8; MAX_VAL], val_len: 0, used: false }
    }
}

struct EnvTable { entries: [EnvEntry; MAX_VARS] }

static mut ENV_TABLE: EnvTable = EnvTable {
    entries: [
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
        EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(), EnvEntry::empty(),
    ],
};

// ── Byte-slice helpers (no implicit autorefs) ─────────────────────────────────

/// Compare two byte slices using raw pointer reads (no reference to the static).
unsafe fn bytes_eq(a_ptr: *const u8, b: &[u8]) -> bool {
    if b.is_empty() { return true; }
    unsafe {
        for i in 0..b.len() {
            if core::ptr::read(a_ptr.add(i)) != b[i] { return false; }
        }
    }
    true
}

/// Copy `src` bytes into `dst_ptr` using raw pointer write.
unsafe fn bytes_copy(dst_ptr: *mut u8, src: &[u8]) {
    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst_ptr, src.len()); }
}

#[inline(always)]
unsafe fn entry_ptr(i: usize) -> *mut EnvEntry {
    unsafe {
        let table: *mut EnvTable = core::ptr::addr_of_mut!(ENV_TABLE);
        core::ptr::addr_of_mut!((*table).entries[i])
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Look up `key` and return a slice of the stored value, or `None`.
pub fn getenv(key: &[u8]) -> Option<&'static [u8]> {
    if key.is_empty() || key.len() > MAX_KEY { return None; }
    unsafe {
        for i in 0..MAX_VARS {
            let ep = entry_ptr(i);
            let used    = core::ptr::read(core::ptr::addr_of!((*ep).used));
            if !used { continue; }
            let key_len = core::ptr::read(core::ptr::addr_of!((*ep).key_len)) as usize;
            if key_len != key.len() { continue; }
            let key_ptr = core::ptr::addr_of!((*ep).key[0]);
            if bytes_eq(key_ptr, key) {
                let val_len = core::ptr::read(core::ptr::addr_of!((*ep).val_len)) as usize;
                let val_ptr = core::ptr::addr_of!((*ep).val[0]);
                return Some(core::slice::from_raw_parts(val_ptr, val_len));
            }
        }
    }
    None
}

/// Set (or create) `key=val`.  Empty `val` removes the key.
/// Returns `true` on success, `false` if the table is full or args are invalid.
pub fn setenv(key: &[u8], val: &[u8]) -> bool {
    if key.is_empty() || key.len() > MAX_KEY || val.len() > MAX_VAL { return false; }
    unsafe {
        // Update or delete an existing entry.
        for i in 0..MAX_VARS {
            let ep = entry_ptr(i);
            let used    = core::ptr::read(core::ptr::addr_of!((*ep).used));
            if !used { continue; }
            let key_len = core::ptr::read(core::ptr::addr_of!((*ep).key_len)) as usize;
            if key_len != key.len() { continue; }
            let key_ptr = core::ptr::addr_of!((*ep).key[0]);
            if !bytes_eq(key_ptr, key) { continue; }
            if val.is_empty() {
                core::ptr::write(core::ptr::addr_of_mut!((*ep).used), false);
            } else {
                let vp = core::ptr::addr_of_mut!((*ep).val[0]);
                bytes_copy(vp, val);
                core::ptr::write(core::ptr::addr_of_mut!((*ep).val_len), val.len() as u16);
            }
            return true;
        }
        if val.is_empty() { return true; }
        // Find a free slot.
        for i in 0..MAX_VARS {
            let ep = entry_ptr(i);
            let used = core::ptr::read(core::ptr::addr_of!((*ep).used));
            if used { continue; }
            let kp = core::ptr::addr_of_mut!((*ep).key[0]);
            bytes_copy(kp, key);
            core::ptr::write(core::ptr::addr_of_mut!((*ep).key_len), key.len() as u8);
            let vp = core::ptr::addr_of_mut!((*ep).val[0]);
            bytes_copy(vp, val);
            core::ptr::write(core::ptr::addr_of_mut!((*ep).val_len), val.len() as u16);
            core::ptr::write(core::ptr::addr_of_mut!((*ep).used), true);
            return true;
        }
    }
    false
}

/// Populate the environment with sensible defaults at kernel boot.
pub fn init_defaults() {
    setenv(b"PATH",     b"/bin");
    setenv(b"HOME",     b"/");
    setenv(b"TERM",     b"vt100");
    setenv(b"USER",     b"oxide");
    setenv(b"SHELL",    b"/bin/sh");
    setenv(b"HOSTNAME", b"oxideos");
}
