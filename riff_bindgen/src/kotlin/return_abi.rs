use super::TypeMapper;
use super::wire;
use crate::model::{Module, ReturnType, Type};

#[derive(Debug, Clone)]
pub enum ReturnAbi {
    Unit,
    Direct {
        kotlin_type: String,
    },
    WireEncoded {
        kotlin_type: String,
        decode_expr: String,
        throws: bool,
    },
}

impl ReturnAbi {
    pub fn from_return_type(returns: &ReturnType, module: &Module) -> Self {
        match returns {
            ReturnType::Void => Self::Unit,
            ReturnType::Value(ty) => Self::from_value_type(ty, module),
            ReturnType::Fallible { ok, err } => Self::from_fallible(ok, err, module),
        }
    }

    fn from_value_type(ty: &Type, module: &Module) -> Self {
        match ty {
            Type::Void => Self::Unit,
            Type::Primitive(_) => Self::Direct {
                kotlin_type: TypeMapper::map_type(ty),
            },
            Type::String
            | Type::Record(_)
            | Type::Enum(_)
            | Type::Vec(_)
            | Type::Option(_)
            | Type::Bytes => Self::WireEncoded {
                kotlin_type: TypeMapper::map_type(ty),
                decode_expr: Self::decode_at_zero(ty, module),
                throws: false,
            },
            _ => Self::Direct {
                kotlin_type: TypeMapper::map_type(ty),
            },
        }
    }

    fn from_fallible(ok: &Type, err: &Type, module: &Module) -> Self {
        let ok_kotlin = TypeMapper::map_type(ok);

        Self::WireEncoded {
            kotlin_type: if ok.is_void() {
                "Unit".into()
            } else {
                ok_kotlin
            },
            decode_expr: Self::result_decode_expr(ok, err, module),
            throws: true,
        }
    }

    fn decode_at_zero(ty: &Type, module: &Module) -> String {
        let codec = wire::decode_type(ty, module);
        codec.value_at("0")
    }

    fn result_decode_expr(ok: &Type, err: &Type, module: &Module) -> String {
        let ok_lambda = Self::ok_decode_lambda(ok, module);
        let err_lambda = Self::err_decode_lambda(err, module);
        let err_to_throwable = Self::err_to_throwable("err", err, module);

        format!(
            "wire.readResult(0, {{ pos -> {} }}, {{ pos -> {} }}).first.unwrapOrThrow {{ err -> {} }}",
            ok_lambda, err_lambda, err_to_throwable
        )
    }

    fn ok_decode_lambda(ok: &Type, module: &Module) -> String {
        if ok.is_void() {
            "Unit to 0".into()
        } else {
            let codec = wire::decode_type(ok, module);
            codec.lambda_body_at("pos")
        }
    }

    fn err_decode_lambda(err: &Type, module: &Module) -> String {
        wire::decode_type(err, module).lambda_body_at("pos")
    }

    fn err_to_throwable(err_value: &str, err_type: &Type, module: &Module) -> String {
        match err_type {
            Type::String => format!("FfiException(-1, {})", err_value),
            Type::Enum(name)
                if module
                    .enums
                    .iter()
                    .any(|enumeration| enumeration.name == *name && enumeration.is_error) =>
            {
                err_value.to_string()
            }
            _ => format!("FfiException(-1, \"Error: ${{{}}}\")", err_value),
        }
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded { .. })
    }

    pub fn throws(&self) -> bool {
        matches!(self, Self::WireEncoded { throws: true, .. })
    }

    pub fn kotlin_type(&self) -> Option<&str> {
        match self {
            Self::Unit => None,
            Self::Direct { kotlin_type } | Self::WireEncoded { kotlin_type, .. } => {
                Some(kotlin_type)
            }
        }
    }

    pub fn decode_expr(&self) -> &str {
        match self {
            Self::WireEncoded { decode_expr, .. } => decode_expr,
            _ => "",
        }
    }

    pub fn jni_return_type(&self) -> &'static str {
        match self {
            Self::Unit => "void",
            Self::Direct { kotlin_type } => match kotlin_type.as_str() {
                "Boolean" => "jboolean",
                "Byte" | "UByte" => "jbyte",
                "Short" | "UShort" => "jshort",
                "Int" | "UInt" => "jint",
                "Long" | "ULong" => "jlong",
                "Float" => "jfloat",
                "Double" => "jdouble",
                _ => "jlong",
            },
            Self::WireEncoded { .. } => "jobject",
        }
    }

    pub fn jni_c_return_type(&self) -> &'static str {
        match self {
            Self::Unit => "void",
            Self::Direct { kotlin_type } => match kotlin_type.as_str() {
                "Boolean" => "bool",
                "Byte" => "int8_t",
                "UByte" => "uint8_t",
                "Short" => "int16_t",
                "UShort" => "uint16_t",
                "Int" => "int32_t",
                "UInt" => "uint32_t",
                "Long" => "int64_t",
                "ULong" => "uint64_t",
                "Float" => "float",
                "Double" => "double",
                _ => "int64_t",
            },
            Self::WireEncoded { .. } => "FfiBuf_u8",
        }
    }

    pub fn jni_result_cast(&self) -> &'static str {
        match self {
            Self::Direct { kotlin_type } => match kotlin_type.as_str() {
                "Boolean" => "(jboolean)",
                "UByte" => "(jbyte)",
                "UShort" => "(jshort)",
                "UInt" => "(jint)",
                "ULong" => "(jlong)",
                _ => "",
            },
            _ => "",
        }
    }

    pub fn kotlin_cast(&self) -> &'static str {
        match self {
            Self::Direct { kotlin_type } => match kotlin_type.as_str() {
                "UByte" => ".toUByte()",
                "UShort" => ".toUShort()",
                "UInt" => ".toUInt()",
                "ULong" => ".toULong()",
                _ => "",
            },
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Primitive;

    #[test]
    fn test_unit_return() {
        let module = Module::new("test");
        let abi = ReturnAbi::from_return_type(&ReturnType::Void, &module);
        assert!(abi.is_unit());
    }

    #[test]
    fn test_primitive_return() {
        let module = Module::new("test");
        let abi = ReturnAbi::from_return_type(
            &ReturnType::Value(Type::Primitive(Primitive::I32)),
            &module,
        );
        assert!(abi.is_direct());
        assert_eq!(abi.kotlin_type(), Some("Int"));
    }

    #[test]
    fn test_string_return_is_wire_encoded() {
        let module = Module::new("test");
        let abi = ReturnAbi::from_return_type(&ReturnType::Value(Type::String), &module);
        assert!(abi.is_wire_encoded());
        assert!(!abi.throws());
    }

    #[test]
    fn test_vec_return_is_wire_encoded() {
        let module = Module::new("test");
        let i32_vec = Type::Vec(Box::new(Type::Primitive(Primitive::I32)));
        let abi = ReturnAbi::from_return_type(&ReturnType::Value(i32_vec), &module);
        assert!(abi.is_wire_encoded());
        assert!(abi.decode_expr().contains("readIntArray"));

        let record_vec = Type::Vec(Box::new(Type::Record("Point".into())));
        let abi2 = ReturnAbi::from_return_type(&ReturnType::Value(record_vec), &module);
        assert!(abi2.is_wire_encoded());
        assert!(abi2.decode_expr().contains("readList"));
    }

    #[test]
    fn test_fallible_is_throwing() {
        let module = Module::new("test");
        let abi = ReturnAbi::from_return_type(
            &ReturnType::Fallible {
                ok: Type::String,
                err: Type::String,
            },
            &module,
        );
        assert!(abi.is_wire_encoded());
        assert!(abi.throws());
    }
}
