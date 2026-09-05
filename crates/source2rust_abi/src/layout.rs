//! ABI layout registry for matching C++ declarations.

use crate::S2RHostApi;

pub struct AbiField {
    pub name: &'static str,
    pub offset: usize,
}

pub struct AbiStruct {
    pub name: &'static str,
    pub size: usize,
    pub align: usize,
    pub fields: &'static [AbiField],
}

macro_rules! abi_struct {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        AbiStruct {
            name: stringify!($ty),
            size: core::mem::size_of::<$ty>(),
            align: core::mem::align_of::<$ty>(),
            fields: &[
                $(
                    AbiField {
                        name: stringify!($field),
                        offset: core::mem::offset_of!($ty, $field),
                    },
                )*
            ],
        }
    };
}

pub const ABI_LAYOUT_REGISTRY: &[AbiStruct] = &[abi_struct!(S2RHostApi {
    version,
    get_interface,
    log,
})];
