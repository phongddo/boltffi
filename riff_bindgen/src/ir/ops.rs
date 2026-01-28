use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ids::{BuiltinId, CustomTypeId, EnumId, FieldName, RecordId};
use crate::ir::types::{PrimitiveType, TypeExpr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireShape {
    Value,
    Optional,
    Sequence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    Instance,
    Var(String),
    Named(String),
    Field(Box<ValueExpr>, FieldName),
}

impl ValueExpr {
    pub fn field(&self, name: FieldName) -> ValueExpr {
        ValueExpr::Field(Box::new(self.clone()), name)
    }

    pub fn remap_root(&self, new_root: ValueExpr) -> ValueExpr {
        match self {
            ValueExpr::Instance | ValueExpr::Var(_) | ValueExpr::Named(_) => new_root,
            ValueExpr::Field(parent, name) => {
                ValueExpr::Field(Box::new(parent.remap_root(new_root)), name.clone())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SizeExpr {
    Fixed(usize),
    Runtime,
    StringLen(ValueExpr),
    BytesLen(ValueExpr),
    ValueSize(ValueExpr),
    WireSize {
        value: ValueExpr,
    },
    BuiltinSize {
        id: BuiltinId,
        value: ValueExpr,
    },
    Sum(Vec<SizeExpr>),
    OptionSize {
        value: ValueExpr,
        inner: Box<SizeExpr>,
    },
    VecSize {
        value: ValueExpr,
        inner: Box<SizeExpr>,
        layout: VecLayout,
    },
    ResultSize {
        value: ValueExpr,
        ok: Box<SizeExpr>,
        err: Box<SizeExpr>,
    },
}

#[derive(Debug, Clone)]
pub struct ReadSeq {
    pub size: SizeExpr,
    pub ops: Vec<ReadOp>,
    pub shape: WireShape,
}

#[derive(Debug, Clone)]
pub struct WriteSeq {
    pub size: SizeExpr,
    pub ops: Vec<WriteOp>,
    pub shape: WireShape,
}

#[derive(Debug, Clone)]
pub enum OffsetExpr {
    Fixed(usize),
    Base,
    BasePlus(usize),
    Var(String),
    VarPlus(String, usize),
}

#[derive(Debug, Clone)]
pub enum ReadOp {
    Primitive {
        primitive: PrimitiveType,
        offset: OffsetExpr,
    },
    String {
        offset: OffsetExpr,
    },
    Bytes {
        offset: OffsetExpr,
    },
    Option {
        tag_offset: OffsetExpr,
        some: Box<ReadSeq>,
    },
    Vec {
        len_offset: OffsetExpr,
        element_type: TypeExpr,
        element: Box<ReadSeq>,
        layout: VecLayout,
    },
    Record {
        id: RecordId,
        offset: OffsetExpr,
        fields: Vec<FieldReadOp>,
    },
    Enum {
        id: EnumId,
        offset: OffsetExpr,
        layout: EnumLayout,
    },
    Result {
        tag_offset: OffsetExpr,
        ok: Box<ReadSeq>,
        err: Box<ReadSeq>,
    },
    Builtin {
        id: BuiltinId,
        offset: OffsetExpr,
    },
    Custom {
        id: CustomTypeId,
        underlying: Box<ReadSeq>,
    },
}

#[derive(Debug, Clone)]
pub enum WriteOp {
    Primitive {
        primitive: PrimitiveType,
        value: ValueExpr,
    },
    String {
        value: ValueExpr,
    },
    Bytes {
        value: ValueExpr,
    },
    Option {
        value: ValueExpr,
        some: Box<WriteSeq>,
    },
    Vec {
        value: ValueExpr,
        element_type: TypeExpr,
        element: Box<WriteSeq>,
        layout: VecLayout,
    },
    Record {
        id: RecordId,
        value: ValueExpr,
        fields: Vec<FieldWriteOp>,
    },
    Enum {
        id: EnumId,
        value: ValueExpr,
        layout: EnumLayout,
    },
    Result {
        value: ValueExpr,
        ok: Box<WriteSeq>,
        err: Box<WriteSeq>,
    },
    Builtin {
        id: BuiltinId,
        value: ValueExpr,
    },
    Custom {
        id: CustomTypeId,
        value: ValueExpr,
        underlying: Box<WriteSeq>,
    },
}

#[derive(Debug, Clone)]
pub struct FieldReadOp {
    pub name: FieldName,
    pub seq: ReadSeq,
}

#[derive(Debug, Clone)]
pub struct FieldWriteOp {
    pub name: FieldName,
    pub accessor: ValueExpr,
    pub seq: WriteSeq,
}

pub fn remap_root_in_seq(seq: &WriteSeq, new_root: ValueExpr) -> WriteSeq {
    WriteSeq {
        size: remap_root_in_size(&seq.size, &new_root),
        ops: seq
            .ops
            .iter()
            .map(|op| remap_root_in_op(op, &new_root))
            .collect(),
        shape: seq.shape,
    }
}

fn remap_root_in_size(size: &SizeExpr, new_root: &ValueExpr) -> SizeExpr {
    match size {
        SizeExpr::Fixed(value) => SizeExpr::Fixed(*value),
        SizeExpr::Runtime => SizeExpr::Runtime,
        SizeExpr::StringLen(value) => SizeExpr::StringLen(value.remap_root(new_root.clone())),
        SizeExpr::BytesLen(value) => SizeExpr::BytesLen(value.remap_root(new_root.clone())),
        SizeExpr::ValueSize(value) => SizeExpr::ValueSize(value.remap_root(new_root.clone())),
        SizeExpr::WireSize { value } => SizeExpr::WireSize {
            value: value.remap_root(new_root.clone()),
        },
        SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
        },
        SizeExpr::Sum(parts) => SizeExpr::Sum(
            parts
                .iter()
                .map(|part| remap_root_in_size(part, new_root))
                .collect(),
        ),
        SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
            value: value.remap_root(new_root.clone()),
            inner: inner.clone(),
        },
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => SizeExpr::VecSize {
            value: value.remap_root(new_root.clone()),
            inner: inner.clone(),
            layout: layout.clone(),
        },
        SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
            value: value.remap_root(new_root.clone()),
            ok: ok.clone(),
            err: err.clone(),
        },
    }
}

fn remap_root_in_op(op: &WriteOp, new_root: &ValueExpr) -> WriteOp {
    match op {
        WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
            primitive: *primitive,
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::String { value } => WriteOp::String {
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::Bytes { value } => WriteOp::Bytes {
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::Option { value, some } => WriteOp::Option {
            value: value.remap_root(new_root.clone()),
            some: some.clone(),
        },
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => WriteOp::Vec {
            value: value.remap_root(new_root.clone()),
            element_type: element_type.clone(),
            element: element.clone(),
            layout: layout.clone(),
        },
        WriteOp::Record { id, value, fields } => WriteOp::Record {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
            fields: fields
                .iter()
                .map(|field| FieldWriteOp {
                    name: field.name.clone(),
                    accessor: field.accessor.remap_root(new_root.clone()),
                    seq: remap_root_in_seq(&field.seq, new_root.clone()),
                })
                .collect(),
        },
        WriteOp::Enum { id, value, layout } => WriteOp::Enum {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
            layout: layout.clone(),
        },
        WriteOp::Result { value, ok, err } => WriteOp::Result {
            value: value.remap_root(new_root.clone()),
            ok: ok.clone(),
            err: err.clone(),
        },
        WriteOp::Builtin { id, value } => WriteOp::Builtin {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::Custom {
            id,
            value,
            underlying,
        } => WriteOp::Custom {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
            underlying: underlying.clone(),
        },
    }
}
