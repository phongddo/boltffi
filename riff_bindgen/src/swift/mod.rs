mod body;
mod conversion;
mod marshal;
mod names;
mod primitives;
mod templates;
mod types;
mod wire;

use askama::Template;

use crate::model::{CallbackTrait, Class, Enumeration, Function, Module, Record, StreamMode};

pub use body::BodyRenderer;
pub use conversion::{CallbackInfo, ParamInfo, ParamsInfo, ReturnInfo};
pub use marshal::{ParamConversion, SwiftType, SyncCallBuilder};
pub use names::NamingConvention;
pub use templates::{
    CStyleEnumTemplate, CallbackTraitTemplate, ClassTemplate, DataEnumTemplate, FunctionTemplate,
    PreambleTemplate, RecordTemplate, StreamCancellableTemplate, StreamSubscriptionTemplate,
};
pub use types::TypeMapper;

pub struct Swift;

impl Swift {
    pub fn render_preamble(module: &Module) -> String {
        PreambleTemplate::for_generator(module)
            .render()
            .expect("preamble template failed")
    }

    pub fn render_record(record: &Record, module: &Module) -> String {
        RecordTemplate::from_record(record, module)
            .render()
            .expect("record template failed")
    }

    pub fn render_enum(enumeration: &Enumeration, module: &Module) -> String {
        if enumeration.is_c_style() {
            CStyleEnumTemplate::from_enum(enumeration)
                .render()
                .expect("c-style enum template failed")
        } else {
            DataEnumTemplate::from_enum(enumeration, module)
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
        Self::render_module_with_ffi_module_name(module, None)
    }

    pub fn render_module_with_ffi_module_name(
        module: &Module,
        ffi_module_name: Option<String>,
    ) -> String {
        let mut sections = Vec::new();

        sections.push(
            ffi_module_name
                .map(|name| PreambleTemplate::for_module_with_ffi_module_name(module, name))
                .unwrap_or_else(|| PreambleTemplate::for_module(module))
                .render()
                .expect("preamble template failed"),
        );

        module
            .records
            .iter()
            .for_each(|r| sections.push(Self::render_record(r, module)));

        module
            .enums
            .iter()
            .for_each(|e| sections.push(Self::render_enum(e, module)));

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

        let mut output = sections
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push('\n');
        output
    }
}
