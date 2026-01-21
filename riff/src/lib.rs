pub use riff_core::{CallbackHandle, CustomFfiConvertible, Data, FfiType, FromCallbackHandle, UnexpectedFfiCallbackError, custom_ffi, data, error, export, name, skip};

#[macro_export]
macro_rules! custom_type {
    (
        $(#[$attrs:meta])*
        $vis:vis $name:ident,
        remote = $remote:path,
        repr = $repr:ty,
        error = $error:ty,
        into_ffi = $into_ffi:expr,
        try_from_ffi = $try_from_ffi:expr $(,)?
    ) => {
        $(#[$attrs])*
        #[repr(transparent)]
        $vis struct $name(pub $remote);

        impl ::core::ops::Deref for $name {
            type Target = $remote;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl ::core::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl ::core::convert::From<$remote> for $name {
            fn from(value: $remote) -> Self {
                Self(value)
            }
        }

        impl ::core::convert::From<$name> for $remote {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        #[::riff::custom_ffi]
        impl ::riff::CustomFfiConvertible for $name {
            type FfiRepr = $repr;
            type Error = $error;

            fn into_ffi(&self) -> Self::FfiRepr {
                let into_ffi_fn = $into_ffi;
                into_ffi_fn(&self.0)
            }

            fn try_from_ffi(repr: Self::FfiRepr) -> Result<Self, Self::Error> {
                let try_from_ffi_fn = $try_from_ffi;
                try_from_ffi_fn(repr).map(Self)
            }
        }
    };
}

#[doc(hidden)]
pub mod __private {
    pub use riff_core::{
        CallbackHandle, EventSubscription, FfiBuf, FfiStatus, FromCallbackHandle,
        RustFutureContinuationCallback, RustFutureHandle, StreamContinuationCallback,
        StreamPollResult, SubscriptionHandle, WaitResult, rustfuture, set_last_error, wire,
    };
}
