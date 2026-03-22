use crate::ir::types::PrimitiveType;

pub fn java_type(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "boolean",
        PrimitiveType::U8 | PrimitiveType::I8 => "byte",
        PrimitiveType::U16 | PrimitiveType::I16 => "short",
        PrimitiveType::U32 | PrimitiveType::I32 => "int",
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::USize | PrimitiveType::ISize => {
            "long"
        }
        PrimitiveType::F32 => "float",
        PrimitiveType::F64 => "double",
    }
}

pub fn java_boxed_type(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "Boolean",
        PrimitiveType::U8 | PrimitiveType::I8 => "Byte",
        PrimitiveType::U16 | PrimitiveType::I16 => "Short",
        PrimitiveType::U32 | PrimitiveType::I32 => "Integer",
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::USize | PrimitiveType::ISize => {
            "Long"
        }
        PrimitiveType::F32 => "Float",
        PrimitiveType::F64 => "Double",
    }
}

pub fn jni_type(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "boolean",
        PrimitiveType::U8 | PrimitiveType::I8 => "byte",
        PrimitiveType::U16 | PrimitiveType::I16 => "short",
        PrimitiveType::U32 | PrimitiveType::I32 => "int",
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::USize | PrimitiveType::ISize => {
            "long"
        }
        PrimitiveType::F32 => "float",
        PrimitiveType::F64 => "double",
    }
}

pub fn java_default_value(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "false",
        PrimitiveType::F32 => "0f",
        PrimitiveType::F64 => "0.0",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "0L"
        }
        _ => "0",
    }
}

pub fn java_primitive_array_type(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "boolean[]",
        PrimitiveType::U8 | PrimitiveType::I8 => "byte[]",
        PrimitiveType::U16 | PrimitiveType::I16 => "short[]",
        PrimitiveType::U32 | PrimitiveType::I32 => "int[]",
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::USize | PrimitiveType::ISize => {
            "long[]"
        }
        PrimitiveType::F32 => "float[]",
        PrimitiveType::F64 => "double[]",
    }
}
