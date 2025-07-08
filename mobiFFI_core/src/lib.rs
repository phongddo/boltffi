#![allow(unused)]

pub mod safety;
pub mod status;
pub mod types;

pub use safety::catch_ffi_panic;
pub use status::FfiStatus;
pub use types::{FfiBuf, FfiSlice, FfiString};

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn mffi_version_major() -> u32 {
    VERSION_MAJOR
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_version_minor() -> u32 {
    VERSION_MINOR
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_version_patch() -> u32 {
    VERSION_PATCH
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_free_buf_u8(buf: FfiBuf<u8>) {
    drop(buf);
}

#[unsafe(no_mangle)]
pub extern "C" fn mffi_free_string(string: FfiString) {
    drop(string);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mffi_copy_bytes(
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
    written: *mut usize,
) -> FfiStatus {
    if src.is_null() || dst.is_null() || written.is_null() {
        return FfiStatus::NULL_POINTER;
    }

    if src_len > dst_cap {
        return FfiStatus::BUFFER_TOO_SMALL;
    }

    core::ptr::copy_nonoverlapping(src, dst, src_len);
    *written = src_len;

    FfiStatus::OK
}
