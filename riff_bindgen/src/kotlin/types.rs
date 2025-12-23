use crate::model::{Primitive, Type};

use super::NamingConvention;

pub struct TypeMapper;

impl TypeMapper {
    pub fn map_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::map_primitive(primitive),
            Type::String => "String".into(),
            Type::Bytes => "ByteArray".into(),
            Type::Slice(inner) => format!("List<{}>", Self::map_type(inner)),
            Type::MutSlice(inner) => format!("MutableList<{}>", Self::map_type(inner)),
            Type::Vec(inner) => format!("List<{}>", Self::map_type(inner)),
            Type::Option(inner) => format!("{}?", Self::map_type(inner)),
            Type::Result { ok, .. } => Self::map_type(ok),
            Type::Callback(inner) => format!("({}) -> Unit", Self::map_type(inner)),
            Type::Object(name) => NamingConvention::class_name(name),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(name) => NamingConvention::class_name(name),
            Type::BoxedTrait(name) => NamingConvention::class_name(name),
            Type::Void => "Unit".into(),
        }
    }

    fn map_primitive(primitive: &Primitive) -> String {
        match primitive {
            Primitive::I8 => "Byte",
            Primitive::I16 => "Short",
            Primitive::I32 => "Int",
            Primitive::I64 => "Long",
            Primitive::U8 => "UByte",
            Primitive::U16 => "UShort",
            Primitive::U32 => "UInt",
            Primitive::U64 => "ULong",
            Primitive::F32 => "Float",
            Primitive::F64 => "Double",
            Primitive::Bool => "Boolean",
            Primitive::Usize => "Long",
            Primitive::Isize => "Long",
        }
        .into()
    }

    pub fn jni_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::jni_primitive(primitive),
            Type::String => "String".into(),
            Type::Bytes => "ByteArray".into(),
            Type::Object(_) | Type::BoxedTrait(_) => "Long".into(),
            Type::Record(name) => NamingConvention::class_name(name),
            Type::Enum(_) => "Int".into(),
            Type::Vec(_) | Type::Slice(_) | Type::MutSlice(_) => "Long".into(),
            Type::Option(inner) => format!("{}?", Self::jni_type(inner)),
            Type::Result { ok, .. } => Self::jni_type(ok),
            Type::Callback(_) => "Long".into(),
            Type::Void => "Unit".into(),
        }
    }

    fn jni_primitive(primitive: &Primitive) -> String {
        match primitive {
            Primitive::I8 => "Byte",
            Primitive::I16 => "Short",
            Primitive::I32 => "Int",
            Primitive::I64 => "Long",
            Primitive::U8 => "Byte",
            Primitive::U16 => "Short",
            Primitive::U32 => "Int",
            Primitive::U64 => "Long",
            Primitive::F32 => "Float",
            Primitive::F64 => "Double",
            Primitive::Bool => "Boolean",
            Primitive::Usize => "Long",
            Primitive::Isize => "Long",
        }
        .into()
    }

    pub fn default_value(ty: &Type) -> String {
        match ty {
            Type::Primitive(primitive) => Self::primitive_default(primitive),
            Type::String => "\"\"".into(),
            Type::Bytes => "byteArrayOf()".into(),
            Type::Vec(_) | Type::Slice(_) | Type::MutSlice(_) => "emptyList()".into(),
            Type::Option(_) => "null".into(),
            Type::Void => "Unit".into(),
            Type::Result { ok, .. } => Self::default_value(ok),
            Type::Object(name) => panic!("no default value for object type '{}'", name),
            Type::Record(name) => panic!("no default value for record type '{}'", name),
            Type::Enum(name) => panic!("no default value for enum type '{}'", name),
            Type::BoxedTrait(name) => panic!("no default value for trait type '{}'", name),
            Type::Callback(_) => panic!("no default value for callback type"),
        }
    }

    fn primitive_default(primitive: &Primitive) -> String {
        match primitive {
            Primitive::Bool => "false",
            Primitive::F32 => "0.0f",
            Primitive::F64 => "0.0",
            Primitive::U8 | Primitive::U16 | Primitive::U32 | Primitive::U64 | Primitive::Usize => {
                "0u"
            }
            Primitive::I8 | Primitive::I16 | Primitive::I32 | Primitive::I64 | Primitive::Isize => {
                "0"
            }
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jni_type_primitives() {
        assert_eq!(TypeMapper::jni_type(&Type::Primitive(Primitive::I32)), "Int");
        assert_eq!(TypeMapper::jni_type(&Type::Primitive(Primitive::U64)), "Long");
        assert_eq!(TypeMapper::jni_type(&Type::Primitive(Primitive::Bool)), "Boolean");
    }

    #[test]
    fn test_jni_type_string_bytes() {
        assert_eq!(TypeMapper::jni_type(&Type::String), "String");
        assert_eq!(TypeMapper::jni_type(&Type::Bytes), "ByteArray");
    }

    #[test]
    fn test_jni_type_object_is_long() {
        assert_eq!(TypeMapper::jni_type(&Type::Object("Sensor".into())), "Long");
        assert_eq!(TypeMapper::jni_type(&Type::BoxedTrait("Handler".into())), "Long");
    }

    #[test]
    fn test_jni_type_vec_is_long() {
        let vec_type = Type::Vec(Box::new(Type::Primitive(Primitive::I32)));
        assert_eq!(TypeMapper::jni_type(&vec_type), "Long");
    }

    #[test]
    fn test_default_value_unsigned() {
        assert_eq!(TypeMapper::default_value(&Type::Primitive(Primitive::U32)), "0u");
        assert_eq!(TypeMapper::default_value(&Type::Primitive(Primitive::U64)), "0u");
    }

    #[test]
    fn test_default_value_signed() {
        assert_eq!(TypeMapper::default_value(&Type::Primitive(Primitive::I32)), "0");
        assert_eq!(TypeMapper::default_value(&Type::Primitive(Primitive::I64)), "0");
    }

    #[test]
    fn test_default_value_collections() {
        assert_eq!(TypeMapper::default_value(&Type::String), "\"\"");
        assert_eq!(TypeMapper::default_value(&Type::Bytes), "byteArrayOf()");
        let vec_type = Type::Vec(Box::new(Type::Primitive(Primitive::I32)));
        assert_eq!(TypeMapper::default_value(&vec_type), "emptyList()");
    }

    #[test]
    fn test_default_value_option() {
        let opt_type = Type::Option(Box::new(Type::String));
        assert_eq!(TypeMapper::default_value(&opt_type), "null");
    }
}
