pub use riff_core::{
    CallbackHandle, CustomFfiConvertible, CustomTypeConversionError, Data, FfiType,
    FromCallbackHandle, UnexpectedFfiCallbackError, custom_ffi, custom_type, data, error, export,
    name, skip,
};

#[doc(hidden)]
pub mod __private {
    pub use riff_core::{
        CallbackHandle, EventSubscription, FfiBuf, FfiStatus, FromCallbackHandle,
        RustFutureContinuationCallback, RustFutureHandle, StreamContinuationCallback,
        StreamPollResult, SubscriptionHandle, WaitResult, rustfuture, set_last_error, wire,
    };
}
