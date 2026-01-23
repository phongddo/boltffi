use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{CustomType, Module};

use super::super::names::NamingConvention;
use super::super::{TypeMapper, wire};

#[derive(Template)]
#[template(path = "swift/preamble.txt", escape = "none")]
pub struct PreambleTemplate {
    pub prefix: String,
    pub ffi_module_name: Option<String>,
    pub has_async: bool,
    pub has_streams: bool,
    pub custom_types: Vec<CustomTypeView>,
}

impl PreambleTemplate {
    pub fn for_generator(module: &Module) -> Self {
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        let has_streams = module.classes.iter().any(|c| !c.streams.is_empty());
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: None,
            has_async,
            has_streams,
            custom_types: module
                .custom_types
                .iter()
                .map(|custom_type| CustomTypeView::from_model(custom_type, module))
                .collect(),
        }
    }

    pub fn for_module(module: &Module) -> Self {
        let ffi_module_name = format!("{}FFI", NamingConvention::class_name(&module.name));
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        let has_streams = module.classes.iter().any(|c| !c.streams.is_empty());
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: Some(ffi_module_name),
            has_async,
            has_streams,
            custom_types: module
                .custom_types
                .iter()
                .map(|custom_type| CustomTypeView::from_model(custom_type, module))
                .collect(),
        }
    }

    pub fn for_module_with_ffi_module_name(module: &Module, ffi_module_name: String) -> Self {
        let has_async = module.functions.iter().any(|function| function.is_async)
            || module
                .classes
                .iter()
                .any(|class_item| class_item.methods.iter().any(|method| method.is_async));
        let has_streams = module.classes.iter().any(|c| !c.streams.is_empty());
        Self {
            prefix: naming::ffi_prefix().to_string(),
            ffi_module_name: Some(ffi_module_name),
            has_async,
            has_streams,
            custom_types: module
                .custom_types
                .iter()
                .map(|custom_type| CustomTypeView::from_model(custom_type, module))
                .collect(),
        }
    }
}

pub struct CustomTypeView {
    pub class_name: String,
    pub repr_swift_type: String,
    pub repr_decode_tuple_expr: String,
    pub repr_size_expr: String,
    pub repr_encode_to_data: String,
    pub repr_encode_to_bytes: String,
}

impl CustomTypeView {
    fn from_model(custom_type: &CustomType, module: &Module) -> Self {
        let class_name = NamingConvention::class_name(&custom_type.name);
        let repr_swift_type = TypeMapper::map_type(&custom_type.repr);

        let repr_decode_tuple_expr =
            wire::decode_type(&custom_type.repr, module).decode_as_tuple("offset");
        let repr_encoder = wire::encode_type(&custom_type.repr, "value", module);

        Self {
            class_name,
            repr_swift_type,
            repr_decode_tuple_expr,
            repr_size_expr: repr_encoder.size_expr,
            repr_encode_to_data: repr_encoder.encode_to_data,
            repr_encode_to_bytes: repr_encoder.encode_to_bytes,
        }
    }
}
