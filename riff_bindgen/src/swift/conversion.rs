use super::names::NamingConvention;
use super::types::TypeMapper;
use crate::model::{BuiltinId, ReturnType, Type};

#[derive(Debug, Clone, Default)]
pub struct ReturnInfo {
    pub swift_type: Option<String>,
    pub is_void: bool,
    pub is_result: bool,
    pub result_ok_type: Option<String>,
}

impl ReturnInfo {
    pub fn from_return_type(returns: &ReturnType) -> Self {
        match returns {
            ReturnType::Void => Self {
                is_void: true,
                ..Default::default()
            },
            ReturnType::Value(ty) => match ty {
                Type::Void => Self {
                    is_void: true,
                    ..Default::default()
                },
                _ => Self {
                    swift_type: Some(TypeMapper::map_type(ty)),
                    ..Default::default()
                },
            },
            ReturnType::Fallible { ok, .. } => {
                let ok_type = match ok {
                    Type::Void => None,
                    _ => Some(TypeMapper::map_type(ok)),
                };
                Self {
                    swift_type: ok_type.clone(),
                    is_result: true,
                    result_ok_type: ok_type,
                    is_void: matches!(ok, Type::Void),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParamInfo {
    pub swift_name: String,
    pub swift_type: String,
    pub ffi_conversion: String,
    pub is_string: bool,
    pub is_slice: bool,
    pub is_mut_slice: bool,
    pub is_vec: bool,
    pub is_vec_wire_encoded: bool,
    pub is_escaping: bool,
}

impl ParamInfo {
    pub fn from_param(name: &str, ty: &Type) -> Self {
        let swift_name = NamingConvention::param_name(name);
        let swift_type = TypeMapper::map_type(ty);
        let is_string = matches!(ty, Type::String);
        let is_slice = matches!(ty, Type::Slice(_));
        let is_mut_slice = matches!(ty, Type::MutSlice(_));
        let is_vec = matches!(ty, Type::Vec(_));
        let is_vec_wire_encoded =
            matches!(ty, Type::Vec(inner) if !matches!(inner.as_ref(), Type::Primitive(_)));
        let is_escaping = matches!(ty, Type::Closure(_));

        let ffi_conversion = match ty {
            Type::Enum(_) => format!("{}.cValue", swift_name),
            Type::BoxedTrait(trait_name) => {
                format!(
                    "{}Bridge.create({})",
                    NamingConvention::class_name(trait_name),
                    swift_name
                )
            }
            Type::Option(inner) => match inner.as_ref() {
                Type::BoxedTrait(trait_name) => format!(
                    "{}.map {{ {}Bridge.create($0) }}",
                    swift_name,
                    NamingConvention::class_name(trait_name)
                ),
                _ => swift_name.clone(),
            },
            _ => swift_name.clone(),
        };

        Self {
            swift_name,
            swift_type,
            ffi_conversion,
            is_string,
            is_slice,
            is_mut_slice,
            is_vec,
            is_vec_wire_encoded,
            is_escaping,
        }
    }

    pub fn needs_wrapper(&self) -> bool {
        self.is_string || self.is_slice || self.is_mut_slice || self.is_vec
    }
}

#[derive(Debug, Clone)]
pub struct CallbackInfo {
    pub param_name: String,
    pub swift_type: String,
    pub ffi_arg_type: String,
    pub context_type: String,
    pub box_type: String,
    pub box_name: String,
    pub ptr_name: String,
    pub trampoline_name: String,
    pub trampoline_args: String,
    pub trampoline_call_args: String,
}

impl CallbackInfo {
    pub fn from_param(name: &str, ty: &Type, func_name_pascal: &str, index: usize) -> Option<Self> {
        let Type::Closure(sig) = ty else {
            return None;
        };

        let param_name = NamingConvention::param_name(name);
        let suffix = if index > 0 {
            format!("{}", index + 1)
        } else {
            String::new()
        };

        let params_swift = sig
            .params
            .iter()
            .map(TypeMapper::map_type)
            .collect::<Vec<_>>()
            .join(", ");

        let params_ffi = sig
            .params
            .iter()
            .map(TypeMapper::ffi_type)
            .collect::<Vec<_>>()
            .join(", ");

        let (arg_names, call_conversions) = sig
            .params
            .iter()
            .enumerate()
            .fold((Vec::new(), Vec::new()), |(mut arg_names, mut call_conversions), (index, ty)| {
                let wire_expr = match ty {
                    Type::Record(name) => Some(format!(
                        "{class_name}.decode(wireBuffer: WireBuffer(ptr: ptr{index}!, len: Int(len{index})), at: 0).value",
                        class_name = NamingConvention::class_name(name),
                        index = index
                    )),
                    Type::String => Some(format!(
                        "WireBuffer(ptr: ptr{index}!, len: Int(len{index})).readString(at: 0).value",
                        index = index
                    )),
                    Type::Builtin(id) => Some(decode_builtin_callback(*id, index)),
                    _ => None,
                };

                if let Some(expr) = wire_expr {
                    arg_names.extend([format!("ptr{}", index), format!("len{}", index)]);
                    call_conversions.push(expr);
                    return (arg_names, call_conversions);
                }

                arg_names.push(format!("val{}", index));
                call_conversions.push(format!("val{}", index));
                (arg_names, call_conversions)
            });

        let trampoline_args = arg_names.join(", ");
        let trampoline_call_args = call_conversions.join(", ");

        Some(Self {
            param_name: param_name.clone(),
            swift_type: params_swift,
            ffi_arg_type: params_ffi,
            context_type: format!("{}CallbackFn{}", func_name_pascal, suffix),
            box_type: format!("{}CallbackBox{}", func_name_pascal, suffix),
            box_name: format!("{}Box{}", param_name, suffix),
            ptr_name: format!("{}Ptr{}", param_name, suffix),
            trampoline_name: format!("{}Trampoline{}", param_name, suffix),
            trampoline_args,
            trampoline_call_args,
        })
    }
}

fn decode_builtin_callback(id: BuiltinId, index: usize) -> String {
    match id {
        BuiltinId::Duration => format!(
            "WireBuffer(ptr: ptr{0}!, len: Int(len{0})).readDuration(at: 0)",
            index
        ),
        BuiltinId::SystemTime => format!(
            "WireBuffer(ptr: ptr{0}!, len: Int(len{0})).readTimestamp(at: 0)",
            index
        ),
        BuiltinId::Uuid => format!(
            "WireBuffer(ptr: ptr{0}!, len: Int(len{0})).readUuid(at: 0)",
            index
        ),
        BuiltinId::Url => format!(
            "WireBuffer(ptr: ptr{0}!, len: Int(len{0})).readUrl(at: 0).value",
            index
        ),
    }
}

pub struct ParamsInfo {
    pub params: Vec<ParamInfo>,
    pub callbacks: Vec<CallbackInfo>,
    pub has_callbacks: bool,
}

impl ParamsInfo {
    pub fn from_inputs<'a>(
        inputs: impl Iterator<Item = (&'a str, &'a Type)>,
        func_name_pascal: &str,
    ) -> Self {
        let mut params = Vec::new();
        let mut callbacks = Vec::new();
        let mut callback_index = 0;

        for (name, ty) in inputs {
            params.push(ParamInfo::from_param(name, ty));

            if matches!(ty, Type::Closure(_))
                && let Some(cb) =
                    CallbackInfo::from_param(name, ty, func_name_pascal, callback_index)
            {
                callbacks.push(cb);
                callback_index += 1;
            }
        }

        let has_callbacks = !callbacks.is_empty();

        Self {
            params,
            callbacks,
            has_callbacks,
        }
    }
}
