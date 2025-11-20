use crate::model::Type;

use super::NamingConvention;

pub struct TypeMapper;

impl TypeMapper {
    pub fn map_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.swift_type().into(),
            Type::String => "String".into(),
            Type::Bytes => "Data".into(),
            Type::Slice(inner) => format!("[{}]", Self::map_type(inner)),
            Type::MutSlice(inner) => format!("[{}]", Self::map_type(inner)),
            Type::Vec(inner) => format!("[{}]", Self::map_type(inner)),
            Type::Option(inner) => format!("{}?", Self::map_type(inner)),
            Type::Result { ok, .. } => Self::map_type(ok),
            Type::Callback(inner) => format!("({}) -> Void", Self::map_type(inner)),
            Type::Object(name) => NamingConvention::class_name(name),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(name) => NamingConvention::class_name(name),
            Type::BoxedTrait(name) => format!("{}Protocol", NamingConvention::class_name(name)),
            Type::Void => "Void".into(),
        }
    }

    pub fn ffi_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.swift_type().into(),
            Type::String => "UnsafePointer<CChar>".into(),
            Type::Bytes => "UnsafePointer<UInt8>".into(),
            Type::Slice(inner) => format!("UnsafePointer<{}>", Self::ffi_type(inner)),
            Type::MutSlice(inner) => format!("UnsafeMutablePointer<{}>", Self::ffi_type(inner)),
            Type::Vec(_) => "UnsafeMutableRawPointer".into(),
            Type::Option(inner) => Self::ffi_type(inner),
            Type::Result { ok, .. } => Self::ffi_type(ok),
            Type::Callback(inner) => {
                format!(
                    "@convention(c) (UnsafeMutableRawPointer?, {}) -> Void",
                    Self::ffi_type(inner)
                )
            }
            Type::Object(_) => "OpaquePointer".into(),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(_) => "Int32".into(),
            Type::BoxedTrait(_) => "OpaquePointer".into(),
            Type::Void => "Void".into(),
        }
    }

    pub fn ffi_type_name(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.rust_name().into(),
            Type::String => "string".into(),
            Type::Record(name) => name.to_lowercase(),
            _ => "unknown".into(),
        }
    }

    pub fn default_value(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.default_value().into(),
            Type::String => "\"\"".into(),
            Type::Bytes => "Data()".into(),
            Type::Vec(_) => "[]".into(),
            Type::Option(_) => "nil".into(),
            Type::Void => "()".into(),
            _ => "/* default */".into(),
        }
    }

    pub fn needs_conversion(ty: &Type) -> bool {
        matches!(
            ty,
            Type::String
                | Type::Bytes
                | Type::Vec(_)
                | Type::Option(_)
                | Type::Object(_)
                | Type::BoxedTrait(_)
        )
    }

    pub fn to_ffi_conversion(param_name: &str, ty: &Type) -> String {
        match ty {
            Type::String => format!("{}", param_name),
            Type::Primitive(_) => param_name.to_string(),
            Type::Record(_) => param_name.to_string(),
            Type::Enum(_) => param_name.to_string(),
            _ => param_name.to_string(),
        }
    }

    pub fn from_ffi_conversion(ty: &Type, expr: &str) -> String {
        match ty {
            Type::String => format!("String(cString: {}.ptr!)", expr),
            _ => expr.to_string(),
        }
    }
}
