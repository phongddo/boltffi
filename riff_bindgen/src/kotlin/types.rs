use crate::model::{BuiltinId, Primitive, Type};

use super::NamingConvention;
use super::primitives;

pub struct TypeMapper;

impl TypeMapper {
    pub fn map_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::map_primitive(primitive),
            Type::String => "String".into(),
            Type::Bytes => "ByteArray".into(),
            Type::Builtin(id) => Self::map_builtin(*id),
            Type::Slice(inner) | Type::Vec(inner) => Self::map_sequence(inner),
            Type::MutSlice(inner) => Self::map_mutable_sequence(inner),
            Type::Option(inner) => format!("{}?", Self::map_type(inner)),
            Type::Result { ok, err } => format!(
                "RiffResult<{}, {}>",
                Self::map_type(ok),
                Self::map_type(err)
            ),
            Type::Closure(sig) => {
                let params = sig
                    .params
                    .iter()
                    .map(Self::map_type)
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = if sig.returns.is_void() {
                    "Unit".to_string()
                } else {
                    Self::map_type(&sig.returns)
                };
                format!("({}) -> {}", params, ret)
            }
            Type::Custom { name, .. } => NamingConvention::class_name(name),
            Type::Object(name) => NamingConvention::class_name(name),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(name) => NamingConvention::class_name(name),
            Type::BoxedTrait(name) => NamingConvention::class_name(name),
            Type::Void => "Unit".into(),
        }
    }

    fn map_builtin(id: BuiltinId) -> String {
        match id {
            BuiltinId::Duration => "Duration".to_string(),
            BuiltinId::SystemTime => "Instant".to_string(),
            BuiltinId::Uuid => "UUID".to_string(),
            BuiltinId::Url => "URI".to_string(),
        }
    }

    fn map_primitive(primitive: &Primitive) -> String {
        primitives::info(*primitive).kotlin_type.into()
    }

    fn map_sequence(inner: &Type) -> String {
        match inner {
            Type::Primitive(p) => Self::primitive_array_type(*p),
            _ => format!("List<{}>", Self::map_type(inner)),
        }
    }

    fn map_mutable_sequence(inner: &Type) -> String {
        match inner {
            Type::Primitive(p) => Self::primitive_array_type(*p),
            _ => format!("MutableList<{}>", Self::map_type(inner)),
        }
    }

    fn primitive_array_type(p: Primitive) -> String {
        match p {
            Primitive::I8 | Primitive::U8 => "ByteArray",
            Primitive::I16 | Primitive::U16 => "ShortArray",
            Primitive::I32 | Primitive::U32 => "IntArray",
            Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize => "LongArray",
            Primitive::F32 => "FloatArray",
            Primitive::F64 => "DoubleArray",
            Primitive::Bool => "BooleanArray",
        }
        .into()
    }

    pub fn jni_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::jni_primitive(primitive),
            Type::String => "String".into(),
            Type::Bytes => "ByteArray".into(),
            Type::Builtin(id) => Self::map_builtin(*id),
            Type::Object(_) | Type::BoxedTrait(_) => "Long".into(),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Custom { name, .. } => NamingConvention::class_name(name),
            Type::Enum(_) => "Int".into(),
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => match inner.as_ref() {
                Type::Primitive(Primitive::I32) => "IntArray".into(),
                Type::Primitive(Primitive::U32) => "IntArray".into(),
                Type::Primitive(Primitive::I16) => "ShortArray".into(),
                Type::Primitive(Primitive::U16) => "ShortArray".into(),
                Type::Primitive(Primitive::I64) => "LongArray".into(),
                Type::Primitive(Primitive::U64) => "LongArray".into(),
                Type::Primitive(Primitive::Isize) => "LongArray".into(),
                Type::Primitive(Primitive::Usize) => "LongArray".into(),
                Type::Primitive(Primitive::F32) => "FloatArray".into(),
                Type::Primitive(Primitive::F64) => "DoubleArray".into(),
                Type::Primitive(Primitive::U8) | Type::Primitive(Primitive::I8) => {
                    "ByteArray".into()
                }
                Type::Primitive(Primitive::Bool) => "BooleanArray".into(),
                Type::Record(_) => "ByteBuffer".into(),
                _ => "Long".into(),
            },
            Type::Option(inner)
                if matches!(inner.as_ref(), Type::Object(_) | Type::BoxedTrait(_)) =>
            {
                "Long".into()
            }
            Type::Option(inner) => format!("{}?", Self::jni_type(inner)),
            Type::Result { ok, .. } => Self::jni_type(ok),
            Type::Closure(sig) => format!("{}Callback", sig.signature_id()),
            Type::Void => "Unit".into(),
        }
    }

    fn jni_primitive(primitive: &Primitive) -> String {
        primitives::info(*primitive).call_suffix.into()
    }

    pub fn c_jni_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::c_jni_primitive(primitive),
            Type::String => "jstring".into(),
            Type::Bytes => "jbyteArray".into(),
            Type::Builtin(_) => "jobject".into(),
            Type::Object(_) | Type::BoxedTrait(_) => "jlong".into(),
            Type::Record(_) => "jlong".into(),
            Type::Custom { .. } => "jlong".into(),
            Type::Enum(_) => "jint".into(),
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => match inner.as_ref() {
                Type::Primitive(p) => primitives::info(*p).array_type.into(),
                Type::Record(_) => "jobject".into(),
                _ => "jlong".into(),
            },
            Type::Option(inner) => Self::c_jni_type(inner),
            Type::Result { ok, .. } => Self::c_jni_type(ok),
            Type::Closure(_) => "jobject".into(),
            Type::Void => "void".into(),
        }
    }

    fn c_jni_primitive(primitive: &Primitive) -> String {
        primitives::info(*primitive).jni_type.into()
    }

    pub fn default_value(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::primitive_default(primitive),
            Type::String => "\"\"".into(),
            Type::Bytes => "byteArrayOf()".into(),
            Type::Builtin(id) => match id {
                BuiltinId::Duration => "Duration.ZERO".into(),
                BuiltinId::SystemTime => "Instant.EPOCH".into(),
                BuiltinId::Uuid => "UUID(0L, 0L)".into(),
                BuiltinId::Url => "URI.create(\"about:blank\")".into(),
            },
            Type::Vec(inner) | Type::Slice(inner) | Type::MutSlice(inner) => {
                if matches!(inner.as_ref(), Type::Primitive(_)) {
                    match inner.as_ref() {
                        Type::Primitive(Primitive::I8 | Primitive::U8) => "byteArrayOf()",
                        Type::Primitive(Primitive::I16 | Primitive::U16) => "shortArrayOf()",
                        Type::Primitive(Primitive::I32 | Primitive::U32) => "intArrayOf()",
                        Type::Primitive(
                            Primitive::I64 | Primitive::U64 | Primitive::Isize | Primitive::Usize,
                        ) => "longArrayOf()",
                        Type::Primitive(Primitive::F32) => "floatArrayOf()",
                        Type::Primitive(Primitive::F64) => "doubleArrayOf()",
                        Type::Primitive(Primitive::Bool) => "booleanArrayOf()",
                        _ => "emptyList()",
                    }
                    .into()
                } else {
                    "emptyList()".into()
                }
            }
            Type::Option(_) => "null".into(),
            Type::Void => "Unit".into(),
            Type::Result { ok, .. } => Self::default_value(ok),
            Type::Object(name) => panic!("no default value for object type '{}'", name),
            Type::Record(name) => panic!("no default value for record type '{}'", name),
            Type::Custom { name, .. } => panic!("no default value for custom type '{}'", name),
            Type::Enum(name) => panic!("no default value for enum type '{}'", name),
            Type::BoxedTrait(name) => panic!("no default value for trait type '{}'", name),
            Type::Closure(_) => panic!("no default value for closure type"),
        }
    }

    fn primitive_default(primitive: &Primitive) -> String {
        primitives::info(*primitive).default_value.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jni_type_primitives() {
        assert_eq!(
            TypeMapper::jni_type(&Type::Primitive(Primitive::I32)),
            "Int"
        );
        assert_eq!(
            TypeMapper::jni_type(&Type::Primitive(Primitive::U64)),
            "Long"
        );
        assert_eq!(
            TypeMapper::jni_type(&Type::Primitive(Primitive::Bool)),
            "Boolean"
        );
    }

    #[test]
    fn test_jni_type_string_bytes() {
        assert_eq!(TypeMapper::jni_type(&Type::String), "String");
        assert_eq!(TypeMapper::jni_type(&Type::Bytes), "ByteArray");
    }

    #[test]
    fn test_jni_type_object_is_long() {
        assert_eq!(TypeMapper::jni_type(&Type::Object("Sensor".into())), "Long");
        assert_eq!(
            TypeMapper::jni_type(&Type::BoxedTrait("Handler".into())),
            "Long"
        );
    }

    #[test]
    fn test_jni_type_vec_primitives() {
        let i32_vec = Type::Vec(Box::new(Type::Primitive(Primitive::I32)));
        assert_eq!(TypeMapper::jni_type(&i32_vec), "IntArray");

        let f64_vec = Type::Vec(Box::new(Type::Primitive(Primitive::F64)));
        assert_eq!(TypeMapper::jni_type(&f64_vec), "DoubleArray");

        let u8_vec = Type::Vec(Box::new(Type::Primitive(Primitive::U8)));
        assert_eq!(TypeMapper::jni_type(&u8_vec), "ByteArray");

        let record_vec = Type::Vec(Box::new(Type::Record("Point".into())));
        assert_eq!(TypeMapper::jni_type(&record_vec), "ByteBuffer");
    }

    #[test]
    fn test_default_value_unsigned() {
        assert_eq!(
            TypeMapper::default_value(&Type::Primitive(Primitive::U32)),
            "0u"
        );
        assert_eq!(
            TypeMapper::default_value(&Type::Primitive(Primitive::U64)),
            "0u"
        );
    }

    #[test]
    fn test_default_value_signed() {
        assert_eq!(
            TypeMapper::default_value(&Type::Primitive(Primitive::I32)),
            "0"
        );
        assert_eq!(
            TypeMapper::default_value(&Type::Primitive(Primitive::I64)),
            "0"
        );
    }

    #[test]
    fn test_default_value_collections() {
        assert_eq!(TypeMapper::default_value(&Type::String), "\"\"");
        assert_eq!(TypeMapper::default_value(&Type::Bytes), "byteArrayOf()");
        let i32_vec = Type::Vec(Box::new(Type::Primitive(Primitive::I32)));
        assert_eq!(TypeMapper::default_value(&i32_vec), "intArrayOf()");
        let record_vec = Type::Vec(Box::new(Type::Record("Point".into())));
        assert_eq!(TypeMapper::default_value(&record_vec), "emptyList()");
    }

    #[test]
    fn test_default_value_option() {
        let opt_type = Type::Option(Box::new(Type::String));
        assert_eq!(TypeMapper::default_value(&opt_type), "null");
    }
}
