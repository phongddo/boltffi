use crate::wire::constants::*;

#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "uuid")]
use uuid::Uuid;

#[cfg(feature = "url")]
use url::Url;

pub trait WireSize {
    fn is_fixed_size() -> bool
    where
        Self: Sized,
    {
        false
    }
    fn fixed_size() -> Option<usize>
    where
        Self: Sized,
    {
        None
    }
    fn wire_size(&self) -> usize;
}

pub trait WireEncode: WireSize {
    fn encode_to(&self, buf: &mut [u8]) -> usize;
}

macro_rules! impl_wire_primitive {
    ($($ty:ty),*) => {
        $(
            impl WireSize for $ty {
                #[inline]
                fn is_fixed_size() -> bool { true }

                #[inline]
                fn fixed_size() -> Option<usize> { Some(core::mem::size_of::<$ty>()) }

                #[inline]
                fn wire_size(&self) -> usize { core::mem::size_of::<$ty>() }
            }

            impl WireEncode for $ty {
                #[inline]
                fn encode_to(&self, buf: &mut [u8]) -> usize {
                    let bytes = self.to_le_bytes();
                    buf[..bytes.len()].copy_from_slice(&bytes);
                    bytes.len()
                }
            }
        )*
    };
}

impl_wire_primitive!(i8, i16, i32, i64, u8, u16, u32, u64, f32, f64);

impl WireSize for bool {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(1)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        1
    }
}

impl WireEncode for bool {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        buf[0] = if *self { 1 } else { 0 };
        1
    }
}

impl WireSize for isize {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(8)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        8
    }
}

impl WireEncode for isize {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let value = *self as i64;
        let bytes = value.to_le_bytes();
        buf[..8].copy_from_slice(&bytes);
        8
    }
}

impl WireSize for usize {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(8)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        8
    }
}

impl WireEncode for usize {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let value = *self as u64;
        let bytes = value.to_le_bytes();
        buf[..8].copy_from_slice(&bytes);
        8
    }
}

impl WireSize for str {
    #[inline]
    fn wire_size(&self) -> usize {
        STRING_LEN_SIZE + self.len()
    }
}

impl WireEncode for str {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let len = self.len() as u32;
        buf[..STRING_LEN_SIZE].copy_from_slice(&len.to_le_bytes());
        buf[STRING_LEN_SIZE..STRING_LEN_SIZE + self.len()].copy_from_slice(self.as_bytes());
        STRING_LEN_SIZE + self.len()
    }
}

impl WireSize for String {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        self.as_str().wire_size()
    }
}

impl WireEncode for String {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        self.as_str().encode_to(buf)
    }
}

impl WireSize for Duration {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(12)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        12
    }
}

impl WireEncode for Duration {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let seconds = self.as_secs();
        let nanos = self.subsec_nanos();
        seconds.encode_to(&mut buf[..8]);
        nanos.encode_to(&mut buf[8..12]);
        12
    }
}

impl WireSize for SystemTime {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(12)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        12
    }
}

impl WireEncode for SystemTime {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let nanos_per_second = 1_000_000_000i128;
        let total_nanos: i128 = match self.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                (duration.as_secs() as i128) * nanos_per_second + (duration.subsec_nanos() as i128)
            }
            Err(error) => {
                let duration = error.duration();
                -((duration.as_secs() as i128) * nanos_per_second
                    + (duration.subsec_nanos() as i128))
            }
        };

        let seconds = total_nanos.div_euclid(nanos_per_second) as i64;
        let nanos = total_nanos.rem_euclid(nanos_per_second) as u32;

        seconds.encode_to(&mut buf[..8]);
        nanos.encode_to(&mut buf[8..12]);
        12
    }
}

#[cfg(feature = "uuid")]
impl WireSize for Uuid {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(16)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        16
    }
}

#[cfg(feature = "uuid")]
impl WireEncode for Uuid {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let bytes = self.as_bytes();
        let hi = u64::from_be_bytes(bytes[..8].try_into().expect("uuid hi bytes"));
        let lo = u64::from_be_bytes(bytes[8..].try_into().expect("uuid lo bytes"));
        hi.encode_to(&mut buf[..8]);
        lo.encode_to(&mut buf[8..16]);
        16
    }
}

#[cfg(feature = "url")]
impl WireSize for Url {
    #[inline]
    fn wire_size(&self) -> usize {
        self.as_str().wire_size()
    }
}

#[cfg(feature = "url")]
impl WireEncode for Url {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        self.as_str().encode_to(buf)
    }
}

#[cfg(feature = "chrono")]
impl WireSize for DateTime<Utc> {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(12)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        12
    }
}

#[cfg(feature = "chrono")]
impl WireEncode for DateTime<Utc> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let seconds = self.timestamp();
        let nanos = self.timestamp_subsec_nanos();
        seconds.encode_to(&mut buf[..8]);
        nanos.encode_to(&mut buf[8..12]);
        12
    }
}

impl<T: WireSize> WireSize for Option<T> {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        match self {
            Some(value) => OPTION_FLAG_SIZE + value.wire_size(),
            None => OPTION_FLAG_SIZE,
        }
    }
}

impl<T: WireEncode> WireEncode for Option<T> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        match self {
            Some(value) => {
                buf[0] = 1;
                OPTION_FLAG_SIZE + value.encode_to(&mut buf[OPTION_FLAG_SIZE..])
            }
            None => {
                buf[0] = 0;
                OPTION_FLAG_SIZE
            }
        }
    }
}

impl<T: WireSize> WireSize for Vec<T> {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        VEC_COUNT_SIZE + self.iter().map(|e| e.wire_size()).sum::<usize>()
    }
}

impl<T: WireEncode> WireEncode for Vec<T> {
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let count = self.len() as u32;
        buf[..VEC_COUNT_SIZE].copy_from_slice(&count.to_le_bytes());
        let mut offset = VEC_COUNT_SIZE;
        for element in self {
            offset += element.encode_to(&mut buf[offset..]);
        }
        offset
    }
}

impl<T: WireSize> WireSize for [T] {
    #[inline]
    fn wire_size(&self) -> usize {
        VEC_COUNT_SIZE + self.iter().map(|e| e.wire_size()).sum::<usize>()
    }
}

impl<T: WireEncode> WireEncode for [T] {
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        let count = self.len() as u32;
        buf[..VEC_COUNT_SIZE].copy_from_slice(&count.to_le_bytes());
        let mut offset = VEC_COUNT_SIZE;
        for element in self {
            offset += element.encode_to(&mut buf[offset..]);
        }
        offset
    }
}

impl<T: WireSize, E: WireSize> WireSize for Result<T, E> {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        match self {
            Ok(value) => RESULT_TAG_SIZE + value.wire_size(),
            Err(err) => RESULT_TAG_SIZE + err.wire_size(),
        }
    }
}

impl<T: WireEncode, E: WireEncode> WireEncode for Result<T, E> {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        match self {
            Ok(value) => {
                buf[0] = 0;
                RESULT_TAG_SIZE + value.encode_to(&mut buf[RESULT_TAG_SIZE..])
            }
            Err(err) => {
                buf[0] = 1;
                RESULT_TAG_SIZE + err.encode_to(&mut buf[RESULT_TAG_SIZE..])
            }
        }
    }
}

impl WireSize for () {
    #[inline]
    fn is_fixed_size() -> bool {
        true
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        Some(0)
    }

    #[inline]
    fn wire_size(&self) -> usize {
        0
    }
}

impl WireEncode for () {
    #[inline]
    fn encode_to(&self, _buf: &mut [u8]) -> usize {
        0
    }
}

impl<T: WireSize + ?Sized> WireSize for &T {
    #[inline]
    fn is_fixed_size() -> bool {
        false
    }

    #[inline]
    fn fixed_size() -> Option<usize> {
        None
    }

    #[inline]
    fn wire_size(&self) -> usize {
        (*self).wire_size()
    }
}

impl<T: WireEncode + ?Sized> WireEncode for &T {
    #[inline]
    fn encode_to(&self, buf: &mut [u8]) -> usize {
        (*self).encode_to(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_primitives() {
        let mut buf = [0u8; 8];

        let written = 42i32.encode_to(&mut buf);
        assert_eq!(written, 4);
        assert_eq!(&buf[..4], &[42, 0, 0, 0]);

        let written = 3.5f64.encode_to(&mut buf);
        assert_eq!(written, 8);

        let written = true.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 1);

        let written = false.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn encode_string() {
        let mut buf = [0u8; 32];
        let s = "hello";

        let written = s.encode_to(&mut buf);
        assert_eq!(written, 9); // 4 (len) + 5 (bytes)
        assert_eq!(&buf[..4], &[5, 0, 0, 0]); // len = 5
        assert_eq!(&buf[4..9], b"hello");
    }

    #[test]
    fn encode_option_some() {
        let mut buf = [0u8; 16];
        let opt: Option<i32> = Some(42);

        let written = opt.encode_to(&mut buf);
        assert_eq!(written, 5); // 1 (flag) + 4 (i32)
        assert_eq!(buf[0], 1); // is_some
        assert_eq!(&buf[1..5], &[42, 0, 0, 0]);
    }

    #[test]
    fn encode_option_none() {
        let mut buf = [0u8; 16];
        let opt: Option<i32> = None;

        let written = opt.encode_to(&mut buf);
        assert_eq!(written, 1);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn encode_vec_fixed_size() {
        let mut buf = [0u8; 32];
        let vec: Vec<i32> = vec![1, 2, 3];

        let written = vec.encode_to(&mut buf);
        assert_eq!(written, 16); // 4 (count) + 3 * 4 (elements)
        assert_eq!(&buf[..4], &[3, 0, 0, 0]); // count = 3
        assert_eq!(&buf[4..8], &[1, 0, 0, 0]);
        assert_eq!(&buf[8..12], &[2, 0, 0, 0]);
        assert_eq!(&buf[12..16], &[3, 0, 0, 0]);
    }

    #[test]
    fn encode_vec_variable_size() {
        let mut buf = [0u8; 64];
        let vec: Vec<String> = vec!["hi".to_string(), "there".to_string()];

        let written = vec.encode_to(&mut buf);
        assert_eq!(written, 4 + 6 + 9);
        assert_eq!(&buf[..4], &[2, 0, 0, 0]);
    }

    #[test]
    fn wire_size_calculations() {
        assert_eq!(42i32.wire_size(), 4);
        assert_eq!("hello".wire_size(), 9);
        assert_eq!(Some(42i32).wire_size(), 5);
        assert_eq!(None::<i32>.wire_size(), 1);

        let vec: Vec<i32> = vec![1, 2, 3];
        assert_eq!(vec.wire_size(), 16);

        let vec: Vec<String> = vec!["hi".to_string(), "there".to_string()];
        assert_eq!(vec.wire_size(), 4 + 6 + 9);
    }
}
