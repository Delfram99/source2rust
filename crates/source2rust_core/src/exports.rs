use source2rust_abi::{ABI_MAGIC, API_VERSION, S2RHostApi};
use source2rust_runtime::panic_guard;

#[unsafe(no_mangle)]
pub extern "C" fn s2r_abi_fingerprint() -> u64 {
    panic_guard::catch_or_default("abi_fingerprint", || ((ABI_MAGIC as u64) << u32::BITS) | u64::from(API_VERSION))
}

#[unsafe(no_mangle)]
/// Starts the Rust core with callbacks supplied by the native host.
///
/// # Safety
///
/// `host` must be aligned and point to a readable [`S2RHostApi`] for this call.
pub unsafe extern "C" fn s2r_core_start(host: *const S2RHostApi) -> bool {
    panic_guard::catch_or_default("core_start", || {
        // SAFETY: the caller guarantees that `host` points to a readable host API for this call.
        let Some(host) = (unsafe { host.as_ref() }).copied() else {
            return false;
        };
        if host.version != API_VERSION as i32 {
            return false;
        }

        crate::start(host)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn s2r_core_stop() {
    panic_guard::catch_or_ignore("core_stop", crate::stop);
}
