#![deny(unsafe_code)]
//! Internal Rust core linked into the native MetaMod plugin.

use source2rust_abi::S2RHostApi;

pub(crate) fn start(host: S2RHostApi) -> bool {
    if !logging::initialize(host) {
        return false;
    }

    log::info!("Rust core loaded successfully.");
    true
}

pub(crate) fn stop() {
    logging::shutdown();
}

#[allow(unsafe_code)]
mod exports;
#[allow(unsafe_code)]
mod logging;
