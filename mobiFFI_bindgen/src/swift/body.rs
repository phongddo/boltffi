use askama::Template;

use crate::model::{Class, Method, Module, StreamMethod, StreamMode};

use super::templates::{
    AsyncMethodBodyTemplate, AsyncThrowingMethodBodyTemplate,
    StreamAsyncBodyTemplate, StreamBatchBodyTemplate, StreamCallbackBodyTemplate,
    SyncMethodBodyTemplate, ThrowingMethodBodyTemplate,
};

pub struct BodyRenderer;

impl BodyRenderer {
    pub fn method(method: &Method, class: &Class, module: &Module) -> String {
        match (method.is_async, method.throws()) {
            (true, true) => AsyncThrowingMethodBodyTemplate::from_method(method, class, module)
                .render()
                .expect("async throwing method template failed"),
            (true, false) => AsyncMethodBodyTemplate::from_method(method, class, module)
                .render()
                .expect("async method template failed"),
            (false, true) => ThrowingMethodBodyTemplate::from_method(method, class, module)
                .render()
                .expect("throwing method template failed"),
            (false, false) => SyncMethodBodyTemplate::from_method(method, class, module)
                .render()
                .expect("sync method template failed"),
        }
    }

    pub fn stream(stream: &StreamMethod, class: &Class, module: &Module) -> String {
        match stream.mode {
            StreamMode::Async => StreamAsyncBodyTemplate::from_stream(stream, class, module)
                .render()
                .expect("stream async body template failed"),
            StreamMode::Batch => StreamBatchBodyTemplate::from_stream(stream, class, module)
                .render()
                .expect("stream batch body template failed"),
            StreamMode::Callback => StreamCallbackBodyTemplate::from_stream(stream, class, module)
                .render()
                .expect("stream callback body template failed"),
        }
    }
}
