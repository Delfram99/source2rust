use core::ffi::{c_char, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct S2RHostApi {
    pub version: i32,
    pub get_interface: unsafe extern "C" fn(name: *const c_char, version: i32) -> *const c_void,
    pub log: unsafe extern "C" fn(level: u32, message: *const c_char),
}
