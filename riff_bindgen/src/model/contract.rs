use std::borrow::Cow;

use super::{BuiltinId, DataEnumLayout, Module, Primitive, ReturnType, Type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContract {
    pub params: Vec<ParamContract>,
    pub returns: ReturnContract,
}

impl CallContract {
    pub fn for_function(
        params: &[super::Parameter],
        returns: &ReturnType,
        module: &Module,
    ) -> Self {
        Self {
            params: params
                .iter()
                .map(|param| ParamContract::for_param(&param.name, &param.param_type, module))
                .collect(),
            returns: ReturnContract::for_return(returns, module),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamContract {
    pub name: String,
    pub transport: ParamTransport,
}

impl ParamContract {
    pub fn for_param(param_name: &str, ty: &Type, module: &Module) -> Self {
        Self {
            name: param_name.to_string(),
            transport: ParamTransport::for_type(ty, module),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamTransport {
    PassThrough(PassThroughType),
    WireEncoded(AbiType),
}

impl ParamTransport {
    pub fn for_type(ty: &Type, module: &Module) -> Self {
        PassThroughType::try_from_param_model(ty)
            .map(Self::PassThrough)
            .unwrap_or_else(|| Self::WireEncoded(AbiType::from_model(ty, module)))
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassThroughType {
    Primitive(Primitive),
    String,
    Bytes,
    PrimitiveVec {
        primitive: Primitive,
    },
    PrimitiveSlice {
        primitive: Primitive,
        mutability: SliceMutability,
    },
    Handle,
    Closure {
        signature_id: String,
    },
}

impl PassThroughType {
    pub fn try_from_model(ty: &Type) -> Option<Self> {
        match ty {
            Type::Primitive(primitive) => Some(Self::Primitive(*primitive)),
            Type::String => Some(Self::String),
            Type::Bytes => Some(Self::Bytes),
            Type::Builtin(_) => None,
            Type::Vec(inner) => inner
                .as_ref()
                .primitive()
                .map(|primitive| Self::PrimitiveVec { primitive }),
            Type::Slice(inner) => {
                inner
                    .as_ref()
                    .primitive()
                    .map(|primitive| Self::PrimitiveSlice {
                        primitive,
                        mutability: SliceMutability::Shared,
                    })
            }
            Type::MutSlice(inner) => {
                inner
                    .as_ref()
                    .primitive()
                    .map(|primitive| Self::PrimitiveSlice {
                        primitive,
                        mutability: SliceMutability::Mutable,
                    })
            }
            Type::Object(_) | Type::BoxedTrait(_) => Some(Self::Handle),
            Type::Closure(signature) => Some(Self::Closure {
                signature_id: signature.signature_id(),
            }),
            Type::Void
            | Type::Custom { .. }
            | Type::Record(_)
            | Type::Enum(_)
            | Type::Option(_)
            | Type::Result { .. } => None,
        }
    }

    pub fn try_from_param_model(ty: &Type) -> Option<Self> {
        match ty {
            Type::Option(inner)
                if matches!(inner.as_ref(), Type::Object(_) | Type::BoxedTrait(_)) =>
            {
                Some(Self::Handle)
            }
            _ => Self::try_from_model(ty),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceMutability {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnContract {
    Unit,
    Direct(PassThroughType),
    WireEncoded {
        value: AbiType,
        throws: bool,
        error: Option<AbiType>,
    },
}

impl ReturnContract {
    pub fn for_return(returns: &ReturnType, module: &Module) -> Self {
        match returns {
            ReturnType::Void => Self::Unit,
            ReturnType::Value(ty) => Self::for_value(ty, module),
            ReturnType::Fallible { ok, err } => Self::WireEncoded {
                value: AbiType::result(ok, err, module),
                throws: true,
                error: Some(AbiType::from_model(err, module)),
            },
        }
    }

    fn for_value(ty: &Type, module: &Module) -> Self {
        match ty {
            Type::Void => Self::Unit,
            _ => PassThroughType::try_from_model(ty)
                .map(Self::Direct)
                .unwrap_or_else(|| Self::WireEncoded {
                    value: AbiType::from_model(ty, module),
                    throws: false,
                    error: None,
                }),
        }
    }

    pub fn throws(&self) -> bool {
        matches!(self, Self::WireEncoded { throws: true, .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiType {
    Unit,
    Primitive(Primitive),
    String,
    Bytes,
    Builtin(BuiltinId),
    Vec(Box<AbiType>),
    Option(Box<AbiType>),
    Result { ok: Box<AbiType>, err: Box<AbiType> },
    Record(RecordRepr),
    Enum(EnumRepr),
}

impl AbiType {
    pub fn from_model(ty: &Type, module: &Module) -> Self {
        match ty {
            Type::Void => Self::Unit,
            Type::Primitive(primitive) => Self::Primitive(*primitive),
            Type::String => Self::String,
            Type::Bytes => Self::Bytes,
            Type::Builtin(id) => Self::Builtin(*id),
            Type::Vec(inner) => Self::Vec(Box::new(Self::from_model(inner, module))),
            Type::Option(inner) => Self::Option(Box::new(Self::from_model(inner, module))),
            Type::Result { ok, err } => Self::Result {
                ok: Box::new(Self::from_model(ok, module)),
                err: Box::new(Self::from_model(err, module)),
            },
            Type::Record(name) => Self::Record(RecordRepr::for_name(name, module)),
            Type::Enum(name) => Self::Enum(EnumRepr::for_name(name, module)),
            Type::Custom { repr, .. } => Self::from_model(repr, module),
            Type::Slice(_)
            | Type::MutSlice(_)
            | Type::Object(_)
            | Type::BoxedTrait(_)
            | Type::Closure(_) => {
                panic!("AbiType not supported for: {ty:?}")
            }
        }
    }

    pub fn result(ok: &Type, err: &Type, module: &Module) -> Self {
        Self::Result {
            ok: Box::new(Self::from_model(ok, module)),
            err: Box::new(Self::from_model(err, module)),
        }
    }

    pub fn name_hint(&self) -> Cow<'_, str> {
        match self {
            Self::Record(record) => Cow::Borrowed(record.name()),
            Self::Enum(enum_repr) => Cow::Borrowed(enum_repr.name()),
            _ => Cow::Borrowed(""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordRepr {
    Blittable { name: String, size: usize },
    WireEncoded { name: String },
}

impl RecordRepr {
    pub fn for_name(name: &str, module: &Module) -> Self {
        module
            .records
            .iter()
            .find(|record| record.name == name)
            .map(|record| {
                if record.is_blittable() {
                    Self::Blittable {
                        name: name.to_string(),
                        size: record.struct_size().as_usize(),
                    }
                } else {
                    Self::WireEncoded {
                        name: name.to_string(),
                    }
                }
            })
            .unwrap_or_else(|| Self::WireEncoded {
                name: name.to_string(),
            })
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Blittable { name, .. } | Self::WireEncoded { name } => name,
        }
    }

    pub fn is_blittable(&self) -> bool {
        matches!(self, Self::Blittable { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnumRepr {
    CStyle { name: String },
    DataEnum { name: String, size: usize },
}

impl EnumRepr {
    pub fn for_name(name: &str, module: &Module) -> Self {
        let enum_def = module
            .enums
            .iter()
            .find(|enumeration| enumeration.name == name);
        let is_data = enum_def.map(|e| e.is_data_enum()).unwrap_or(false);

        if is_data {
            let size = enum_def
                .and_then(DataEnumLayout::from_enum)
                .map(|layout| layout.struct_size().as_usize())
                .unwrap_or(0);
            Self::DataEnum {
                name: name.to_string(),
                size,
            }
        } else {
            Self::CStyle {
                name: name.to_string(),
            }
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::CStyle { name } | Self::DataEnum { name, .. } => name,
        }
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self, Self::DataEnum { .. })
    }
}
