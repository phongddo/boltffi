mod body;
mod names;
mod templates;
mod types;

use askama::Template;

use crate::model::{CallbackTrait, Class, Enumeration, Function, Module, Record, StreamMode};

pub use body::BodyRenderer;
pub use names::NamingConvention;
pub use templates::{
    CStyleEnumTemplate, CallbackTraitTemplate, ClassTemplate, DataEnumTemplate, FunctionTemplate,
    RecordTemplate, StreamCancellableTemplate, StreamSubscriptionTemplate,
};
pub use types::TypeMapper;

pub struct Swift;

impl Swift {
    pub fn render_record(record: &Record) -> String {
        RecordTemplate::from_record(record)
            .render()
            .expect("record template failed")
    }

    pub fn render_enum(enumeration: &Enumeration) -> String {
        if enumeration.is_c_style() {
            CStyleEnumTemplate::from_enum(enumeration)
                .render()
                .expect("c-style enum template failed")
        } else {
            DataEnumTemplate::from_enum(enumeration)
                .render()
                .expect("data enum template failed")
        }
    }

    pub fn render_class(class: &Class, module: &Module) -> String {
        ClassTemplate::from_class(class, module)
            .render()
            .expect("class template failed")
    }

    pub fn render_stream_wrappers(class: &Class, module: &Module) -> String {
        class
            .streams
            .iter()
            .filter_map(|stream| match stream.mode {
                StreamMode::Batch => Some(
                    StreamSubscriptionTemplate::from_stream(stream, class, module)
                        .render()
                        .expect("subscription template failed"),
                ),
                StreamMode::Callback => Some(
                    StreamCancellableTemplate::from_stream(stream, class, module)
                        .render()
                        .expect("cancellable template failed"),
                ),
                StreamMode::Async => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn render_callback_trait(callback_trait: &CallbackTrait, module: &Module) -> String {
        CallbackTraitTemplate::from_trait(callback_trait, module)
            .render()
            .expect("callback trait template failed")
    }

    pub fn render_function(function: &Function, module: &Module) -> String {
        FunctionTemplate::from_function(function, module)
            .render()
            .expect("function template failed")
    }

    pub fn render_module(module: &Module) -> String {
        let mut sections = Vec::new();

        sections.push(Self::render_header());

        module
            .records
            .iter()
            .for_each(|r| sections.push(Self::render_record(r)));

        module
            .enums
            .iter()
            .for_each(|e| sections.push(Self::render_enum(e)));

        module.classes.iter().for_each(|c| {
            sections.push(Self::render_class(c, module));
            let stream_wrappers = Self::render_stream_wrappers(c, module);
            if !stream_wrappers.is_empty() {
                sections.push(stream_wrappers);
            }
        });

        module
            .callback_traits
            .iter()
            .for_each(|t| sections.push(Self::render_callback_trait(t, module)));

        module
            .functions
            .iter()
            .for_each(|f| sections.push(Self::render_function(f, module)));

        sections.join("\n\n")
    }

    fn render_header() -> String {
        r#"import Foundation

public struct FfiError: Error {
    public let code: Int32
    public let message: String
    
    init(code: Int32, message: String = "FFI Error") {
        self.code = code
        self.message = message
    }
}"#
        .to_string()
    }
}
