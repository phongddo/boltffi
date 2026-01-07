use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Primitive {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Usize,
    Isize,
}

impl Primitive {
    pub fn swift_type(self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::I8 => "Int8",
            Self::U8 => "UInt8",
            Self::I16 => "Int16",
            Self::U16 => "UInt16",
            Self::I32 => "Int32",
            Self::U32 => "UInt32",
            Self::I64 => "Int64",
            Self::U64 => "UInt64",
            Self::F32 => "Float",
            Self::F64 => "Double",
            Self::Usize => "UInt",
            Self::Isize => "Int",
        }
    }

    pub fn rust_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Usize => "usize",
            Self::Isize => "isize",
        }
    }

    pub fn cbindgen_name(self) -> &'static str {
        self.rust_name()
    }

    pub fn c_type_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "int8_t",
            Self::U8 => "uint8_t",
            Self::I16 => "int16_t",
            Self::U16 => "uint16_t",
            Self::I32 => "int32_t",
            Self::U32 => "uint32_t",
            Self::I64 => "int64_t",
            Self::U64 => "uint64_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Usize => "uintptr_t",
            Self::Isize => "intptr_t",
        }
    }

    pub fn default_value(self) -> &'static str {
        match self {
            Self::Bool => "false",
            Self::F32 | Self::F64 => "0.0",
            _ => "0",
        }
    }

    pub fn is_integer(self) -> bool {
        !matches!(self, Self::F32 | Self::F64 | Self::Bool)
    }

    pub fn is_floating_point(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::Isize
        )
    }

    pub fn is_unsigned(self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::Usize)
    }

    pub fn fits_in_32_bits(self) -> bool {
        matches!(
            self,
            Self::Bool | Self::I8 | Self::U8 | Self::I16 | Self::U16 | Self::I32 | Self::U32 | Self::F32
        )
    }

    pub fn jni_array_type(self) -> &'static str {
        match self {
            Self::Bool => "jbooleanArray",
            Self::I8 | Self::U8 => "jbyteArray",
            Self::I16 | Self::U16 => "jshortArray",
            Self::I32 | Self::U32 => "jintArray",
            Self::I64 | Self::U64 | Self::Isize | Self::Usize => "jlongArray",
            Self::F32 => "jfloatArray",
            Self::F64 => "jdoubleArray",
        }
    }

    pub fn jni_new_array_fn(self) -> &'static str {
        match self {
            Self::Bool => "NewBooleanArray",
            Self::I8 | Self::U8 => "NewByteArray",
            Self::I16 | Self::U16 => "NewShortArray",
            Self::I32 | Self::U32 => "NewIntArray",
            Self::I64 | Self::U64 | Self::Isize | Self::Usize => "NewLongArray",
            Self::F32 => "NewFloatArray",
            Self::F64 => "NewDoubleArray",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    Primitive(Primitive),
    String,
    Bytes,
    Slice(Box<Type>),
    MutSlice(Box<Type>),
    Vec(Box<Type>),
    Option(Box<Type>),
    Result { ok: Box<Type>, err: Box<Type> },
    Callback(Box<Type>),
    Object(String),
    Record(String),
    Enum(String),
    BoxedTrait(String),
    Void,
}

impl Type {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, Self::Primitive(_))
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result { .. })
    }

    pub fn inner_type(&self) -> Option<&Type> {
        match self {
            Self::Vec(inner) | Self::Option(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn result_types(&self) -> Option<(&Type, &Type)> {
        match self {
            Self::Result { ok, err } => Some((ok, err)),
            _ => None,
        }
    }

    pub fn named_type(&self) -> Option<&str> {
        match self {
            Self::Object(name) | Self::Record(name) | Self::Enum(name) => Some(name),
            _ => None,
        }
    }

    pub fn vec(element: Type) -> Self {
        Self::Vec(Box::new(element))
    }

    pub fn option(inner: Type) -> Self {
        Self::Option(Box::new(inner))
    }

    pub fn result(ok: Type, err: Type) -> Self {
        Self::Result {
            ok: Box::new(ok),
            err: Box::new(err),
        }
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum(_))
    }

    pub fn is_vec(&self) -> bool {
        matches!(self, Self::Vec(_))
    }

    pub fn primitive(&self) -> Option<Primitive> {
        match self {
            Self::Primitive(p) => Some(*p),
            _ => None,
        }
    }

    pub fn record_name(&self) -> Option<&str> {
        match self {
            Self::Record(name) => Some(name),
            _ => None,
        }
    }

    pub fn enum_name(&self) -> Option<&str> {
        match self {
            Self::Enum(name) => Some(name),
            _ => None,
        }
    }

    pub fn vec_inner(&self) -> Option<&Type> {
        match self {
            Self::Vec(inner) => Some(inner),
            _ => None,
        }
    }
}

use super::layout::{CLayout, Layout};

impl CLayout for Type {
    fn c_layout(&self) -> Layout {
        match self {
            Self::Primitive(primitive) => primitive.c_layout(),
            Self::String | Self::Bytes | Self::Vec(_) | Self::Slice(_) | Self::MutSlice(_) => {
                Layout::new(24, 8)
            }
            Self::Object(_) | Self::BoxedTrait(_) | Self::Callback(_) => Layout::new(8, 8),
            Self::Record(_) | Self::Enum(_) => Layout::new(8, 8),
            Self::Option(inner) => {
                let inner_layout = inner.c_layout();
                Layout::new(
                    inner_layout.size.as_usize() + inner_layout.alignment.as_usize(),
                    inner_layout.alignment.as_usize(),
                )
            }
            Self::Result { ok, .. } => ok.c_layout(),
            Self::Void => Layout::new(0, 1),
        }
    }
}

impl CLayout for Primitive {
    fn c_layout(&self) -> Layout {
        match self {
            Self::Bool | Self::I8 | Self::U8 => Layout::new(1, 1),
            Self::I16 | Self::U16 => Layout::new(2, 2),
            Self::I32 | Self::U32 | Self::F32 => Layout::new(4, 4),
            Self::I64 | Self::U64 | Self::F64 | Self::Usize | Self::Isize => Layout::new(8, 8),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Receiver {
    None,
    Ref,
    RefMut,
}

impl Receiver {
    pub fn is_static(self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_mutable(self) -> bool {
        matches!(self, Self::RefMut)
    }

    pub fn takes_self(self) -> bool {
        !self.is_static()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Deprecation {
    pub message: Option<String>,
    pub since: Option<String>,
}

impl Deprecation {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            since: None,
        }
    }

    pub fn with_since(mut self, version: impl Into<String>) -> Self {
        self.since = Some(version.into());
        self
    }
}
