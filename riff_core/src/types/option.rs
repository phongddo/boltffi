use core::mem::MaybeUninit;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FfiOption<T: Copy> {
    is_some: bool,
    value: MaybeUninit<T>,
}

impl<T: Copy> FfiOption<T> {
    pub fn some(value: T) -> Self {
        Self {
            is_some: true,
            value: MaybeUninit::new(value),
        }
    }

    pub fn none() -> Self {
        Self {
            is_some: false,
            value: MaybeUninit::uninit(),
        }
    }

    pub fn is_some(&self) -> bool {
        self.is_some
    }

    pub fn is_none(&self) -> bool {
        !self.is_some
    }

    pub fn into_option(self) -> Option<T> {
        if self.is_some {
            Some(unsafe { self.value.assume_init() })
        } else {
            None
        }
    }
}

impl<T: Copy> From<Option<T>> for FfiOption<T> {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Self::some(v),
            None => Self::none(),
        }
    }
}

impl<T: Copy> From<FfiOption<T>> for Option<T> {
    fn from(opt: FfiOption<T>) -> Self {
        opt.into_option()
    }
}

impl<T: Copy> Default for FfiOption<T> {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_some() {
        let opt = FfiOption::some(42u32);
        assert!(opt.is_some());
        assert_eq!(opt.into_option(), Some(42));
    }

    #[test]
    fn option_none() {
        let opt: FfiOption<u32> = FfiOption::none();
        assert!(opt.is_none());
        assert_eq!(opt.into_option(), None);
    }
}
