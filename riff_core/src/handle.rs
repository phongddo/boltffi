use std::ptr::NonNull;

pub struct HandleBox<T> {
    ptr: NonNull<T>,
}

impl<T> HandleBox<T> {
    pub fn new(value: T) -> Self {
        let boxed = Box::new(value);
        Self {
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) },
        }
    }

    pub fn into_raw(self) -> *mut T {
        let ptr = self.ptr.as_ptr();
        core::mem::forget(self);
        ptr
    }

    pub unsafe fn from_raw(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| Self { ptr })
    }

    pub fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }

    pub fn as_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for HandleBox<T> {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.ptr.as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_roundtrip() {
        let handle = HandleBox::new(42u32);
        let ptr = handle.into_raw();
        let recovered = unsafe { HandleBox::from_raw(ptr) }.unwrap();
        assert_eq!(*recovered.as_ref(), 42);
    }

    #[test]
    fn handle_null() {
        let result: Option<HandleBox<u32>> = unsafe { HandleBox::from_raw(core::ptr::null_mut()) };
        assert!(result.is_none());
    }
}
