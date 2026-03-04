use crate::types::FfiBuf;
use crate::types::FfiSpan;
use crate::wire::{WireDecode, WireEncode};

pub unsafe trait Passable: Sized {
    type In;
    type Out;
    unsafe fn unpack(input: Self::In) -> Self;
    fn pack(self) -> Self::Out;
}

pub struct Seal;

pub trait VecTransport<T> {
    fn pack(vec: Vec<T>) -> FfiBuf;
    unsafe fn unpack(ptr: *const u8, byte_len: usize) -> Vec<T>;
}

macro_rules! impl_passable_scalar {
    ($($ty:ty),*) => {
        $(
            unsafe impl Passable for $ty {
                type In = $ty;
                type Out = $ty;
                unsafe fn unpack(input: $ty) -> Self { input }
                fn pack(self) -> $ty { self }
            }
        )*
    };
}

macro_rules! impl_vec_direct {
    ($($ty:ty),*) => {
        $(
            impl VecTransport<$ty> for Seal {
                fn pack(vec: Vec<$ty>) -> FfiBuf {
                    FfiBuf::from_vec(vec)
                }
                unsafe fn unpack(ptr: *const u8, byte_len: usize) -> Vec<$ty> {
                    let count = byte_len / core::mem::size_of::<$ty>();
                    unsafe { core::slice::from_raw_parts(ptr as *const $ty, count) }.to_vec()
                }
            }
        )*
    };
}

impl_passable_scalar!(
    i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, bool, usize, isize
);
impl_vec_direct!(
    i8, i16, i32, i64, u16, u32, u64, f32, f64, bool, usize, isize
);

impl VecTransport<u8> for Seal {
    fn pack(vec: Vec<u8>) -> FfiBuf {
        FfiBuf::from_vec(vec)
    }
    unsafe fn unpack(ptr: *const u8, byte_len: usize) -> Vec<u8> {
        unsafe { core::slice::from_raw_parts(ptr, byte_len) }.to_vec()
    }
}

unsafe impl Passable for String {
    type In = FfiSpan;
    type Out = FfiBuf;

    unsafe fn unpack(input: FfiSpan) -> Self {
        let bytes = unsafe { input.as_bytes() };
        core::str::from_utf8(bytes)
            .expect("invalid UTF-8 in FfiSpan")
            .to_string()
    }

    fn pack(self) -> FfiBuf {
        FfiBuf::from_vec(self.into_bytes())
    }
}

pub unsafe trait WirePassable: WireEncode + WireDecode + Sized {}

unsafe impl<T: WirePassable> Passable for T {
    type In = FfiSpan;
    type Out = FfiBuf;

    unsafe fn unpack(input: FfiSpan) -> Self {
        let bytes = unsafe { input.as_bytes() };
        crate::wire::decode(bytes).expect("wire decode failed in Passable::unpack")
    }

    fn pack(self) -> FfiBuf {
        FfiBuf::wire_encode(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_roundtrip() {
        let value: i32 = 42;
        let packed = value.pack();
        let unpacked = unsafe { i32::unpack(packed) };
        assert_eq!(unpacked, 42);
    }

    #[test]
    fn bool_roundtrip() {
        assert!(unsafe { bool::unpack(true.pack()) });
        assert!(!unsafe { bool::unpack(false.pack()) });
    }

    #[test]
    fn string_pack() {
        let value = String::from("hello");
        let buf = value.pack();
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn string_roundtrip() {
        let original = String::from("hello world");
        let bytes = original.as_bytes();
        let span = FfiSpan {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        };
        let recovered = unsafe { String::unpack(span) };
        assert_eq!(recovered, "hello world");
    }
}
