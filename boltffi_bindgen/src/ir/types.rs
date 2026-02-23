use boltffi_ffi_rules::classification::FieldPrimitive;

use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, QualifiedName, RecordId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    F32,
    F64,
}

impl PrimitiveType {
    pub const fn size_bytes(self) -> Option<usize> {
        match self {
            Self::Bool | Self::I8 | Self::U8 => Some(1),
            Self::I16 | Self::U16 => Some(2),
            Self::I32 | Self::U32 | Self::F32 => Some(4),
            Self::I64 | Self::U64 | Self::F64 => Some(8),
            Self::ISize | Self::USize => None,
        }
    }

    pub const fn alignment(self) -> Option<usize> {
        self.size_bytes()
    }

    pub const fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::ISize
        )
    }

    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub const fn is_platform_sized(self) -> bool {
        matches!(self, Self::ISize | Self::USize)
    }

    pub fn to_field_primitive(self) -> FieldPrimitive {
        if self.is_platform_sized() {
            FieldPrimitive::platform_sized()
        } else {
            FieldPrimitive::fixed()
        }
    }

    pub const fn wire_size_bytes(self) -> usize {
        match self {
            Self::Bool | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 | Self::ISize | Self::USize => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Void,
    Primitive(PrimitiveType),
    String,
    Bytes,

    Vec(Box<TypeExpr>),
    Option(Box<TypeExpr>),
    Result {
        ok: Box<TypeExpr>,
        err: Box<TypeExpr>,
    },

    Record(RecordId),
    Enum(EnumId),
    Callback(CallbackId),
    Custom(CustomTypeId),
    Builtin(BuiltinId),

    Handle(ClassId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinKind {
    Duration,
    SystemTime,
    Uuid,
    Url,
}

#[derive(Debug, Clone)]
pub struct BuiltinDef {
    pub id: BuiltinId,
    pub kind: BuiltinKind,
    pub rust_type: QualifiedName,
}
