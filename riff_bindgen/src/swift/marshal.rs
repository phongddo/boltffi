use crate::model::{Module, OptionInfo, Primitive, Type};
use riff_ffi_rules::naming;

use super::names::NamingConvention;

#[derive(Debug, Clone)]
pub struct OptionView {
    pub info: OptionInfo,
    pub inner_type: String,
    pub is_struct: bool,
    pub is_data_enum: bool,
    pub struct_size: usize,
    pub buf_type: String,
    pub free_fn: String,
}

impl OptionView {
    pub fn from_type(inner: &Type, _func_name: &str, module: &Module) -> Self {
        let info = OptionInfo::from_type(inner);
        let is_data_enum = info.is_data_enum(module);
        let struct_size = info.struct_size(module);

        let swift_inner = SwiftType::from_model(inner);
        let (inner_type, is_struct) = match &swift_inner {
            SwiftType::Vec(vec_inner) => (vec_inner.swift_type(), vec_inner.is_struct()),
            other => (other.swift_type(), other.is_struct()),
        };

        let (buf_type, free_fn) = if let SwiftType::Vec(vec_inner) = &swift_inner {
            let cbindgen_name = vec_inner.cbindgen_name();
            (
                format!("FfiBuf_{}", cbindgen_name),
                format!("{}_free_buf_{}", naming::ffi_prefix(), cbindgen_name),
            )
        } else {
            (String::new(), String::new())
        };

        Self {
            info,
            inner_type,
            is_struct,
            is_data_enum,
            struct_size,
            buf_type,
            free_fn,
        }
    }

    pub fn is_vec(&self) -> bool {
        self.info.is_vec
    }

    pub fn is_scalar(&self) -> bool {
        !self.info.is_vec
    }

    pub fn is_packed(&self) -> bool {
        !self.info.is_vec && self.info.inner.primitive().map(|p| p.fits_in_32_bits()).unwrap_or(false)
    }

    pub fn is_large_primitive(&self) -> bool {
        !self.info.is_vec && self.info.inner.primitive().map(|p| !p.fits_in_32_bits()).unwrap_or(false)
    }

    pub fn is_string(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_string()
    }

    pub fn is_record(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_record()
    }

    pub fn is_enum(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_enum() && !self.is_data_enum
    }

    pub fn is_data_enum(&self) -> bool {
        !self.info.is_vec && self.info.inner.is_enum() && self.is_data_enum
    }

    pub fn is_vec_string(&self) -> bool {
        self.info.is_vec && self.info.inner.vec_inner().map(|t| t.is_string()).unwrap_or(false)
    }

    pub fn is_vec_enum(&self) -> bool {
        self.info.is_vec && self.info.inner.vec_inner().map(|t| t.is_enum() && !self.is_data_enum).unwrap_or(false)
    }

    pub fn ffi_option_type(&self) -> &str {
        &self.info.ffi_type
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SwiftType {
    Void,
    Primitive(Primitive),
    String,
    Bytes,
    Slice {
        inner: Box<SwiftType>,
        mutable: bool,
    },
    Vec(Box<SwiftType>),
    Option(Box<SwiftType>),
    Result {
        ok: Box<SwiftType>,
    },
    Enum(String),
    Record(String),
    Object(String),
    BoxedTrait(String),
    Callback(Box<SwiftType>),
}

impl SwiftType {
    pub fn from_model(ty: &Type) -> Self {
        match ty {
            Type::Void => Self::Void,
            Type::Primitive(p) => Self::Primitive(*p),
            Type::String => Self::String,
            Type::Bytes => Self::Bytes,
            Type::Slice(inner) => Self::Slice {
                inner: Box::new(Self::from_model(inner)),
                mutable: false,
            },
            Type::MutSlice(inner) => Self::Slice {
                inner: Box::new(Self::from_model(inner)),
                mutable: true,
            },
            Type::Vec(inner) => Self::Vec(Box::new(Self::from_model(inner))),
            Type::Option(inner) => Self::Option(Box::new(Self::from_model(inner))),
            Type::Result { ok, .. } => Self::Result {
                ok: Box::new(Self::from_model(ok)),
            },
            Type::Enum(name) => Self::Enum(name.clone()),
            Type::Record(name) => Self::Record(name.clone()),
            Type::Object(name) => Self::Object(name.clone()),
            Type::BoxedTrait(name) => Self::BoxedTrait(name.clone()),
            Type::Callback(inner) => Self::Callback(Box::new(Self::from_model(inner))),
        }
    }

    pub fn swift_type(&self) -> String {
        match self {
            Self::Void => "Void".into(),
            Self::Primitive(p) => p.swift_type().into(),
            Self::String => "String".into(),
            Self::Bytes => "Data".into(),
            Self::Slice { inner, .. } | Self::Vec(inner) => format!("[{}]", inner.swift_type()),
            Self::Option(inner) => format!("{}?", inner.swift_type()),
            Self::Result { ok } => ok.swift_type(),
            Self::Enum(name) | Self::Record(name) | Self::Object(name) => {
                NamingConvention::class_name(name)
            }
            Self::BoxedTrait(name) => format!("{}Protocol", NamingConvention::class_name(name)),
            Self::Callback(inner) => format!("({}) -> Void", inner.swift_type()),
        }
    }

    pub fn default_value(&self) -> String {
        match self {
            Self::Void => "()".into(),
            Self::Primitive(p) => p.default_value().into(),
            Self::String => "\"\"".into(),
            Self::Bytes => "Data()".into(),
            Self::Slice { .. } | Self::Vec(_) => "[]".into(),
            Self::Option(_) => "nil".into(),
            Self::Result { ok } => ok.default_value(),
            Self::Enum(_) => "0".into(),
            Self::Record(name) => format!("{}()", NamingConvention::class_name(name)),
            Self::Object(_) | Self::BoxedTrait(_) => "nil".into(),
            Self::Callback(_) => "{ _ in }".into(),
        }
    }

    pub fn ffi_type_suffix(&self) -> String {
        match self {
            Self::Primitive(p) => p.rust_name().into(),
            Self::String => "string".into(),
            Self::Record(name) | Self::Enum(name) => name.to_lowercase(),
            Self::Vec(inner) => inner.ffi_type_suffix(),
            Self::Result { ok } => ok.ffi_type_suffix(),
            _ => "unknown".into(),
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_struct(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    pub fn cbindgen_name(&self) -> String {
        match self {
            Self::Primitive(p) => p.cbindgen_name().to_string(),
            Self::String => "FfiString".to_string(),
            Self::Record(name) | Self::Enum(name) => name.clone(),
            _ => "unknown".to_string(),
        }
    }

    pub fn unwrap_result(&self) -> &SwiftType {
        match self {
            Self::Result { ok } => ok.as_ref(),
            other => other,
        }
    }

    pub fn inner_type(&self) -> Option<&SwiftType> {
        match self {
            Self::Vec(inner) | Self::Option(inner) | Self::Result { ok: inner } => Some(inner),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReturnKind {
    Void,
    Direct,
    String,
    Enum {
        type_name: String,
    },
    Record {
        type_name: String,
    },
    Vec {
        inner_type: String,
        is_struct: bool,
        buf_type: String,
        free_fn: String,
    },
    Option(OptionView),
    Result {
        ok_type: String,
        ok_is_vec: bool,
    },
}

impl ReturnKind {
    pub fn from_function(return_type: Option<&Type>, func_name: &str, module: &Module) -> Self {
        match return_type {
            None => Self::Void,
            Some(Type::Option(inner)) => {
                Self::Option(OptionView::from_type(inner, func_name, module))
            }
            Some(ty) => Self::from_type(ty, func_name),
        }
    }

    fn from_type(ty: &Type, _func_name: &str) -> Self {
        let swift_ty = SwiftType::from_model(ty);
        match swift_ty {
            SwiftType::Void => Self::Void,
            SwiftType::String => Self::String,
            SwiftType::Enum(name) => Self::Enum {
                type_name: NamingConvention::class_name(&name),
            },
            SwiftType::Record(name) => Self::Record {
                type_name: NamingConvention::class_name(&name),
            },
            SwiftType::Vec(inner) => {
                let cbindgen_name = inner.cbindgen_name();
                Self::Vec {
                    inner_type: inner.swift_type(),
                    is_struct: inner.is_struct(),
                    buf_type: format!("FfiBuf_{}", cbindgen_name),
                    free_fn: format!("{}_free_buf_{}", naming::ffi_prefix(), cbindgen_name),
                }
            }
            SwiftType::Result { ok } => Self::Result {
                ok_type: ok.swift_type(),
                ok_is_vec: matches!(ok.as_ref(), SwiftType::Vec(_)),
            },
            _ => Self::Direct,
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum { .. })
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record { .. })
    }

    pub fn is_vec(&self) -> bool {
        matches!(self, Self::Vec { .. })
    }

    pub fn is_option_vec(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_vec())
    }

    pub fn is_option(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub fn is_option_scalar(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_scalar())
    }

    pub fn is_option_packed(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_packed())
    }

    pub fn is_option_large_primitive(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_large_primitive())
    }

    pub fn is_option_string(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_string())
    }

    pub fn is_option_record(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_record())
    }

    pub fn is_option_enum(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_enum())
    }

    pub fn is_option_data_enum(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_data_enum())
    }

    pub fn option_view(&self) -> Option<&OptionView> {
        match self {
            Self::Option(view) => Some(view),
            _ => None,
        }
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result { .. })
    }

    pub fn result_ok_is_vec(&self) -> bool {
        matches!(
            self,
            Self::Result {
                ok_is_vec: true,
                ..
            }
        )
    }

    pub fn throws(&self) -> bool {
        matches!(self, Self::Result { .. })
    }

    pub fn type_name(&self) -> Option<&str> {
        match self {
            Self::Enum { type_name } | Self::Record { type_name } => Some(type_name),
            _ => None,
        }
    }

    pub fn inner_type(&self) -> Option<&str> {
        match self {
            Self::Vec { inner_type, .. } => Some(inner_type),
            Self::Option(opt) => Some(&opt.inner_type),
            Self::Result { ok_type, .. } => Some(ok_type),
            _ => None,
        }
    }

    pub fn vec_is_struct(&self) -> bool {
        matches!(
            self,
            Self::Vec {
                is_struct: true,
                ..
            }
        )
    }

    pub fn vec_buf_type(&self) -> Option<&str> {
        match self {
            Self::Vec { buf_type, .. } => Some(buf_type),
            _ => None,
        }
    }

    pub fn vec_free_fn(&self) -> Option<&str> {
        match self {
            Self::Vec { free_fn, .. } => Some(free_fn),
            _ => None,
        }
    }

    pub fn option_vec_is_struct(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_vec() && opt.is_struct)
    }

    pub fn option_vec_buf_type(&self) -> Option<&str> {
        match self {
            Self::Option(opt) if opt.is_vec() => Some(&opt.buf_type),
            _ => None,
        }
    }

    pub fn option_vec_free_fn(&self) -> Option<&str> {
        match self {
            Self::Option(opt) if opt.is_vec() => Some(&opt.free_fn),
            _ => None,
        }
    }

    pub fn option_vec_is_string(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_vec_string())
    }

    pub fn option_vec_is_enum(&self) -> bool {
        matches!(self, Self::Option(opt) if opt.is_vec_enum())
    }
}

#[derive(Debug, Clone)]
pub struct ParamConversion {
    pub name: String,
    pub swift_type: String,
    pub wrapper_pre: Option<String>,
    pub wrapper_post: Option<String>,
    pub ffi_args: Vec<String>,
    pub is_mutable: bool,
    pub is_escaping: bool,
}

impl ParamConversion {
    pub fn from_param(name: &str, ty: &Type) -> Self {
        let swift_ty = SwiftType::from_model(ty);
        let swift_name = NamingConvention::param_name(name);

        let (wrapper_pre, ffi_args, wrapper_post, is_mutable) = match &swift_ty {
            SwiftType::String => (
                Some(format!(
                    "{}.withCString {{ {}Ptr in",
                    swift_name, swift_name
                )),
                vec![
                    format!(
                        "UnsafeRawPointer({}Ptr).assumingMemoryBound(to: UInt8.self)",
                        swift_name
                    ),
                    format!("UInt({}.utf8.count)", swift_name),
                ],
                Some("}".into()),
                false,
            ),
            SwiftType::Bytes => (
                Some(format!(
                    "{}.withUnsafeBytes {{ {}Ptr in",
                    swift_name, swift_name
                )),
                vec![
                    format!(
                        "{}Ptr.baseAddress!.assumingMemoryBound(to: UInt8.self)",
                        swift_name
                    ),
                    format!("UInt({}.count)", swift_name),
                ],
                Some("}".into()),
                false,
            ),
            SwiftType::Slice { mutable: false, .. } | SwiftType::Vec(_) => (
                Some(format!(
                    "{}.withUnsafeBufferPointer {{ {}Ptr in",
                    swift_name, swift_name
                )),
                vec![
                    format!("{}Ptr.baseAddress", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
                false,
            ),
            SwiftType::Slice { mutable: true, .. } => (
                Some(format!(
                    "{}.withUnsafeMutableBufferPointer {{ {}Ptr in",
                    swift_name, swift_name
                )),
                vec![
                    format!("{}Ptr.baseAddress", swift_name),
                    format!("UInt({}Ptr.count)", swift_name),
                ],
                Some("}".into()),
                true,
            ),
            SwiftType::Enum(_) => (None, vec![format!("{}.cValue", swift_name)], None, false),
            SwiftType::BoxedTrait(trait_name) => (
                None,
                vec![format!(
                    "{}Bridge.create({})",
                    NamingConvention::class_name(trait_name),
                    swift_name
                )],
                None,
                false,
            ),
            _ => (None, vec![swift_name.clone()], None, false),
        };

        Self {
            name: swift_name,
            swift_type: swift_ty.swift_type(),
            wrapper_pre,
            wrapper_post,
            ffi_args,
            is_mutable,
            is_escaping: matches!(swift_ty, SwiftType::Callback(_)),
        }
    }

    pub fn needs_wrapper(&self) -> bool {
        self.wrapper_pre.is_some()
    }
}

pub struct SyncCallBuilder {
    params: Vec<ParamConversion>,
    include_handle: bool,
}

impl SyncCallBuilder {
    pub fn new(_ffi_name: &str, include_handle: bool) -> Self {
        Self {
            params: Vec::new(),
            include_handle,
        }
    }

    pub fn with_params<'a>(mut self, params: impl Iterator<Item = (&'a str, &'a Type)>) -> Self {
        self.params = params
            .map(|(n, t)| ParamConversion::from_param(n, t))
            .collect();
        self
    }

    pub fn has_wrappers(&self) -> bool {
        self.params.iter().any(|p| p.needs_wrapper())
    }

    pub fn build_wrappers_open(&self) -> String {
        self.params
            .iter()
            .filter_map(|p| p.wrapper_pre.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn build_wrappers_close(&self) -> String {
        self.params
            .iter()
            .filter_map(|p| p.wrapper_post.as_ref())
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn build_ffi_args(&self) -> String {
        [if self.include_handle {
            Some("handle")
        } else {
            None
        }]
        .into_iter()
        .flatten()
        .map(String::from)
        .chain(self.params.iter().flat_map(|p| p.ffi_args.clone()))
        .collect::<Vec<_>>()
        .join(", ")
    }
}
