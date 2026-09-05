use std::ffi::CString;
use std::sync::{Condvar, Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};
use source2rust_abi::{LOG_ERROR, LOG_INFO, LOG_WARN, S2RHostApi};

static LOGGER: CoreLogger = CoreLogger;
static LOGGER_READY: OnceLock<bool> = OnceLock::new();
static HOST: Mutex<HostState> = Mutex::new(HostState {
    api: None,
    active_calls: 0,
});
static HOST_IDLE: Condvar = Condvar::new();

struct HostState {
    api: Option<S2RHostApi>,
    active_calls: usize,
}

struct HostCall {
    api: S2RHostApi,
}

impl HostCall {
    fn begin() -> Option<Self> {
        let mut state = HOST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let api = state.api?;
        state.active_calls += 1;
        Some(Self { api })
    }
}

impl Drop for HostCall {
    fn drop(&mut self) {
        let mut state = HOST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active_calls -= 1;
        if state.active_calls == 0 {
            HOST_IDLE.notify_all();
        }
    }
}

struct CoreLogger;

impl Log for CoreLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let Some(host) = HostCall::begin() else {
            return;
        };

        let level = match record.level() {
            Level::Error => LOG_ERROR,
            Level::Warn => LOG_WARN,
            _ => LOG_INFO,
        };

        if let Ok(message) = CString::new(record.args().to_string()) {
            // SAFETY: `message` is null-terminated and remains alive for the callback.
            unsafe { (host.api.log)(level, message.as_ptr()) };
        }
    }

    fn flush(&self) {}
}

pub(crate) fn initialize(host: S2RHostApi) -> bool {
    if !*LOGGER_READY.get_or_init(|| log::set_logger(&LOGGER).is_ok()) {
        return false;
    }

    let mut state = HOST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.api.is_some() || state.active_calls != 0 {
        return false;
    }

    state.api = Some(host);
    drop(state);

    log::set_max_level(LevelFilter::Info);

    true
}

pub(crate) fn shutdown() {
    let mut state = HOST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.api = None;

    while state.active_calls != 0 {
        state = HOST_IDLE.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}
