use askama::Template;
use riff_ffi_rules::naming;

use crate::model::{CallbackTrait, Module, Type};

use super::super::names::NamingConvention;
use super::super::types::TypeMapper;
use super::super::wire;

#[derive(Template)]
#[template(path = "swift/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate {
    pub doc: Option<String>,
    pub protocol_name: String,
    pub wrapper_class: String,
    pub vtable_var: String,
    pub vtable_type: String,
    pub bridge_name: String,
    pub foreign_type: String,
    pub register_fn: String,
    pub create_fn: String,
    pub methods: Vec<TraitMethodView>,
}

pub struct TraitMethodView {
    pub swift_name: String,
    pub ffi_name: String,
    pub params: Vec<TraitParamView>,
    pub return_type: Option<String>,
    pub is_async: bool,
    pub throws: bool,
    pub has_return: bool,
    pub has_out_param: bool,
    pub wire_encoded_return: bool,
    pub wire_return_encode: Option<String>,
}

pub struct TraitParamView {
    pub label: String,
    pub swift_type: String,
    pub ffi_args: Vec<String>,
    pub call_arg: String,
    pub decode_prelude: Option<String>,
}

impl CallbackTraitTemplate {
    pub fn from_trait(callback_trait: &CallbackTrait, module: &Module) -> Self {
        let trait_name = &callback_trait.name;

        Self {
            doc: callback_trait.doc.clone(),
            protocol_name: trait_name.clone(),
            wrapper_class: format!("{}Wrapper", trait_name),
            vtable_var: format!("{}VTableInstance", to_camel_case(trait_name)),
            vtable_type: naming::callback_vtable_name(trait_name),
            bridge_name: format!("{}Bridge", trait_name),
            foreign_type: naming::callback_foreign_name(trait_name),
            register_fn: naming::callback_register_fn(trait_name),
            create_fn: naming::callback_create_fn(trait_name),
            methods: callback_trait
                .methods
                .iter()
                .map(|method| {
                    let has_return = method.has_return();
                    TraitMethodView {
                        swift_name: NamingConvention::method_name(&method.name),
                        ffi_name: naming::to_snake_case(&method.name),
                        params: method
                            .inputs
                            .iter()
                            .map(|param| {
                                let swift_name = NamingConvention::param_name(&param.name);
                                let ffi_args = callback_ffi_args(&swift_name, &param.param_type);
                                let decode_prelude = (ffi_args.len() == 2).then(|| {
                                    format!(
                                        "let {} = {}",
                                        swift_name,
                                        wire_decode_expr(
                                            &param.param_type,
                                            module,
                                            &ffi_args[0],
                                            &ffi_args[1]
                                        )
                                    )
                                });
                                TraitParamView {
                                    label: swift_name.clone(),
                                    swift_type: TypeMapper::map_type(&param.param_type),
                                    ffi_args,
                                    call_arg: swift_name,
                                    decode_prelude,
                                }
                            })
                            .collect(),
                        return_type: method.returns.ok_type().map(TypeMapper::map_type),
                        is_async: method.is_async,
                        throws: method.throws(),
                        has_return,
                        has_out_param: has_return && !method.is_async,
                        wire_encoded_return: method
                            .returns
                            .ok_type()
                            .map(Self::is_wire_encoded_return)
                            .unwrap_or(false),
                        wire_return_encode: method
                            .returns
                            .ok_type()
                            .filter(|ty| Self::is_wire_encoded_return(ty))
                            .map(|ty| wire_encode_data_expr(ty, module, "result")),
                    }
                })
                .collect(),
        }
    }

    fn is_wire_encoded_return(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Record(_)
                | Type::Custom { .. }
                | Type::Enum(_)
                | Type::String
                | Type::Bytes
                | Type::Builtin(_)
                | Type::Vec(_)
                | Type::Option(_)
                | Type::Result { .. }
        )
    }
}

fn callback_ffi_args(base: &str, ty: &Type) -> Vec<String> {
    matches!(ty, Type::Primitive(_) | Type::Void)
        .then_some(vec![base.to_string()])
        .unwrap_or_else(|| vec![format!("{}Ptr", base), format!("{}Len", base)])
}

fn wire_decode_expr(ty: &Type, module: &Module, ptr_name: &str, len_name: &str) -> String {
    let codec = wire::decode_type(ty, module);
    let value_expr = codec.value_at("0");
    format!(
        "({{ let wire = WireBuffer(ptr: {ptr_name}!, len: Int({len_name})); return {value_expr} }})()",
        ptr_name = ptr_name,
        len_name = len_name,
        value_expr = value_expr
    )
}

fn wire_encode_data_expr(ty: &Type, module: &Module, value_name: &str) -> String {
    let encoder = wire::encode_type(ty, value_name, module);
    format!(
        "let encoded = ({{ var data = Data(capacity: {size}); {encode}; return data }})()",
        size = encoder.size_expr,
        encode = encoder.encode_to_data
    )
}

fn to_camel_case(name: &str) -> String {
    name.chars()
        .enumerate()
        .map(|(index, ch)| {
            if index == 0 {
                ch.to_ascii_lowercase()
            } else {
                ch
            }
        })
        .collect()
}
