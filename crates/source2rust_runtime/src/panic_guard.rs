use std::panic::{AssertUnwindSafe, catch_unwind};

pub fn catch_or_default<T: Default>(context: &str, operation: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(value) => value,
        Err(_) => {
            log::error!("panic caught at the Rust FFI boundary: {context}");
            T::default()
        }
    }
}

pub fn catch_or_ignore(context: &str, operation: impl FnOnce()) {
    catch_or_default(context, operation);
}
