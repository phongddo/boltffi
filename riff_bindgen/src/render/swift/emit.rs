use riff_ffi_rules::naming::to_upper_camel_case as pascal_case;

use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ids::BuiltinId;
use crate::ir::ops::{OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};
use riff_ffi_rules::naming::snake_to_camel as camel_case;

const SWIFT_KEYWORDS: &[&str] = &[
    "associatedtype",
    "borrowing",
    "class",
    "consuming",
    "deinit",
    "enum",
    "extension",
    "fileprivate",
    "func",
    "import",
    "init",
    "inout",
    "internal",
    "let",
    "nonisolated",
    "open",
    "operator",
    "precedencegroup",
    "private",
    "protocol",
    "public",
    "rethrows",
    "static",
    "struct",
    "subscript",
    "typealias",
    "var",
    "break",
    "case",
    "catch",
    "continue",
    "default",
    "defer",
    "do",
    "else",
    "fallthrough",
    "for",
    "guard",
    "if",
    "in",
    "repeat",
    "return",
    "switch",
    "throw",
    "where",
    "while",
    "Any",
    "as",
    "await",
    "false",
    "is",
    "nil",
    "self",
    "Self",
    "super",
    "throws",
    "true",
    "try",
    "_",
];

pub fn escape_swift_keyword(name: &str) -> String {
    if SWIFT_KEYWORDS.contains(&name) {
        format!("`{}`", name)
    } else {
        name.to_string()
    }
}

pub fn render_value(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::Instance => "self".to_string(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => camel_case(name),
        ValueExpr::Field(parent, field) => {
            format!("{}.{}", render_value(parent), camel_case(field.as_str()))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadReturn {
    BareValue(usize),
    WithSize,
}

pub fn swift_type(type_expr: &TypeExpr) -> String {
    match type_expr {
        TypeExpr::Void => "Void".to_string(),
        TypeExpr::Primitive(primitive) => swift_primitive(*primitive),
        TypeExpr::String => "String".to_string(),
        TypeExpr::Bytes => "Data".to_string(),
        TypeExpr::Builtin(id) => swift_builtin(id),
        TypeExpr::Option(inner) => format!("{}?", swift_type(inner)),
        TypeExpr::Vec(inner) => {
            if matches!(inner.as_ref(), TypeExpr::Primitive(PrimitiveType::U8)) {
                "Data".to_string()
            } else {
                format!("[{}]", swift_type(inner))
            }
        }
        TypeExpr::Result { ok, err } => {
            format!("Result<{}, {}>", swift_type(ok), swift_type(err))
        }
        TypeExpr::Record(id) => pascal_case(id.as_str()),
        TypeExpr::Enum(id) => pascal_case(id.as_str()),
        TypeExpr::Custom(id) => pascal_case(id.as_str()),
        TypeExpr::Handle(id) => pascal_case(id.as_str()),
        TypeExpr::Callback(id) => pascal_case(id.as_str()),
    }
}

pub fn swift_primitive(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "Bool",
        PrimitiveType::I8 => "Int8",
        PrimitiveType::U8 => "UInt8",
        PrimitiveType::I16 => "Int16",
        PrimitiveType::U16 => "UInt16",
        PrimitiveType::I32 => "Int32",
        PrimitiveType::U32 => "UInt32",
        PrimitiveType::I64 => "Int64",
        PrimitiveType::U64 => "UInt64",
        PrimitiveType::ISize => "Int",
        PrimitiveType::USize => "UInt",
        PrimitiveType::F32 => "Float",
        PrimitiveType::F64 => "Double",
    }
    .to_string()
}

pub fn swift_builtin(id: &BuiltinId) -> String {
    match id.as_str() {
        "Duration" => "TimeInterval",
        "SystemTime" => "Date",
        "Uuid" => "UUID",
        "Url" => "URL",
        other => other,
    }
    .to_string()
}

fn swift_builtin_size_expr(id: &BuiltinId, value: &str) -> String {
    match id.as_str() {
        "Url" => format!("{}.absoluteString.utf8.count", value),
        _ => format!("{}.wireEncodedSize()", value),
    }
}

pub fn emit_read_inline(seq: &ReadSeq, base: &str) -> String {
    let (reader, decode_return) = emit_read_expr(seq, base, base);
    match decode_return {
        ReadReturn::BareValue(size) => {
            format!("{{ let v = {}; {} += {}; return v }}()", reader, base, size)
        }
        ReadReturn::WithSize => {
            format!("{{ let (v, s) = {}; {} += s; return v }}()", reader, base)
        }
    }
}

pub fn emit_read_value_at(seq: &ReadSeq, offset_expr: &str) -> String {
    let (reader, decode_return) = emit_read_expr(seq, "pos", offset_expr);
    match decode_return {
        ReadReturn::BareValue(_) => reader,
        ReadReturn::WithSize => format!("{}.value", reader),
    }
}

pub fn emit_result_ok_throw(ok: &ReadSeq, err: &ReadSeq, err_is_string: bool) -> String {
    let ok_expr = emit_read_value_at(ok, "pos");
    let err_expr = emit_read_value_at(err, "pos");
    let err_body = if err_is_string {
        format!("FfiError(message: {})", err_expr)
    } else {
        err_expr
    };
    format!(
        "try wire.readResultOrThrow(at: 0, ok: {{ pos in {} }}, err: {{ pos in {} }})",
        ok_expr, err_body
    )
}

pub fn emit_read_with_offset(seq: &ReadSeq, base: &str, offset_expr: &str) -> String {
    let (reader, decode_return) = emit_read_expr(seq, base, offset_expr);
    match decode_return {
        ReadReturn::BareValue(_) => reader,
        ReadReturn::WithSize => format!("{}.value", reader),
    }
}

pub fn emit_size_expr(size: &SizeExpr) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::Runtime => "0".to_string(),
        SizeExpr::StringLen(value) => format!("{}.utf8.count", render_value(value)),
        SizeExpr::BytesLen(value) => format!("{}.count", render_value(value)),
        SizeExpr::ValueSize(expr) => render_value(expr),
        SizeExpr::WireSize { value } => format!("{}.wireEncodedSize()", render_value(value)),
        SizeExpr::BuiltinSize { id, value } => swift_builtin_size_expr(id, &render_value(value)),
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(emit_size_expr)
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({})", rendered)
        }
        SizeExpr::OptionSize { value, inner } => {
            let inner_size = emit_size_expr(inner);
            format!(
                "({}.map {{ v in 1 + {} }} ?? 1)",
                render_value(value),
                inner_size
            )
        }
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => {
            let v = render_value(value);
            let inner_size = emit_size_expr(inner);
            match layout {
                VecLayout::Blittable { element_size } => {
                    format!("(4 + {}.count * {})", v, element_size)
                }
                VecLayout::Encoded => {
                    if inner_size.contains("item") {
                        let reduced = inner_size.replace("item", "$1");
                        format!("(4 + {}.reduce(0) {{ $0 + {} }})", v, reduced)
                    } else {
                        format!("(4 + {}.count * {})", v, inner_size)
                    }
                }
            }
        }
        SizeExpr::ResultSize { value, ok, err } => {
            let v = render_value(value);
            let ok_size = emit_size_expr(ok);
            let err_size = emit_size_expr(err);
            let ok_fixed = ok_size.chars().all(|c| c.is_ascii_digit());
            let err_fixed = err_size.chars().all(|c| c.is_ascii_digit());
            let ok_binding = if ok_fixed { "_" } else { "let okVal" };
            let err_binding = if err_fixed { "_" } else { "let errVal" };
            format!(
                "({{ switch {} {{ case .success({}): return 1 + {}; case .failure({}): return 1 + {} }} }}())",
                v, ok_binding, ok_size, err_binding, err_size
            )
        }
    }
}

pub fn emit_write_data(seq: &WriteSeq) -> String {
    seq.ops
        .iter()
        .map(emit_write_data_op)
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn emit_write_bytes(seq: &WriteSeq) -> String {
    seq.ops
        .iter()
        .map(emit_write_bytes_op)
        .collect::<Vec<_>>()
        .join("; ")
}

fn emit_read_expr(seq: &ReadSeq, base_name: &str, base_expr: &str) -> (String, ReadReturn) {
    seq.ops
        .first()
        .map(|op| emit_read_op(op, base_name, base_expr))
        .unwrap_or_else(|| ("()".to_string(), ReadReturn::BareValue(0)))
}

fn emit_read_op(op: &ReadOp, base_name: &str, base_expr: &str) -> (String, ReadReturn) {
    match op {
        ReadOp::Primitive { primitive, offset } => {
            let offset_expr = emit_offset_expr(offset, base_name, base_expr);
            let reader = match primitive {
                PrimitiveType::Bool => format!("wire.readBool(at: {})", offset_expr),
                PrimitiveType::I8 => format!("wire.readI8(at: {})", offset_expr),
                PrimitiveType::U8 => format!("wire.readU8(at: {})", offset_expr),
                PrimitiveType::I16 => format!("wire.readI16(at: {})", offset_expr),
                PrimitiveType::U16 => format!("wire.readU16(at: {})", offset_expr),
                PrimitiveType::I32 => format!("wire.readI32(at: {})", offset_expr),
                PrimitiveType::U32 => format!("wire.readU32(at: {})", offset_expr),
                PrimitiveType::I64 => format!("wire.readI64(at: {})", offset_expr),
                PrimitiveType::U64 => format!("wire.readU64(at: {})", offset_expr),
                PrimitiveType::ISize => format!("Int(wire.readI64(at: {}))", offset_expr),
                PrimitiveType::USize => format!("UInt(wire.readU64(at: {}))", offset_expr),
                PrimitiveType::F32 => format!("wire.readF32(at: {})", offset_expr),
                PrimitiveType::F64 => format!("wire.readF64(at: {})", offset_expr),
            };
            (reader, ReadReturn::BareValue(primitive.wire_size_bytes()))
        }
        ReadOp::String { offset } => {
            let offset_expr = emit_offset_expr(offset, base_name, base_expr);
            (
                format!("wire.readString(at: {})", offset_expr),
                ReadReturn::WithSize,
            )
        }
        ReadOp::Bytes { offset } => {
            let offset_expr = emit_offset_expr(offset, base_name, base_expr);
            (
                format!("wire.readDataWithSize(at: {})", offset_expr),
                ReadReturn::WithSize,
            )
        }
        ReadOp::Builtin { id, offset } => {
            let offset_expr = emit_offset_expr(offset, base_name, base_expr);
            match id.as_str() {
                "Duration" => (
                    format!("wire.readDuration(at: {})", offset_expr),
                    ReadReturn::BareValue(12),
                ),
                "SystemTime" => (
                    format!("wire.readTimestamp(at: {})", offset_expr),
                    ReadReturn::BareValue(12),
                ),
                "Uuid" => (
                    format!("wire.readUuid(at: {})", offset_expr),
                    ReadReturn::BareValue(16),
                ),
                "Url" => (
                    format!("wire.readUrl(at: {})", offset_expr),
                    ReadReturn::WithSize,
                ),
                other => (
                    format!("wire.read{}(at: {})", pascal_case(other), offset_expr),
                    ReadReturn::WithSize,
                ),
            }
        }
        ReadOp::Option { tag_offset, some } => {
            let offset_expr = emit_offset_expr(tag_offset, base_name, base_expr);
            let (inner_reader, inner_return) = emit_read_expr(some, "pos", "$0");
            let tuple_reader = match inner_return {
                ReadReturn::BareValue(size) => format!("({}, {})", inner_reader, size),
                ReadReturn::WithSize => inner_reader,
            };
            (
                format!(
                    "wire.readOptional(at: {}, reader: {{ {} }})",
                    offset_expr, tuple_reader
                ),
                ReadReturn::WithSize,
            )
        }
        ReadOp::Vec {
            len_offset,
            element_type,
            element,
            layout,
        } => {
            let offset_expr = emit_offset_expr(len_offset, base_name, base_expr);
            if matches!(element_type, TypeExpr::Primitive(PrimitiveType::U8)) {
                return (
                    format!("wire.readDataWithSize(at: {})", offset_expr),
                    ReadReturn::WithSize,
                );
            }
            match layout {
                VecLayout::Blittable { .. } => (
                    format!(
                        "wire.readBlittableArrayWithSize(at: {}, as: {}.self)",
                        offset_expr,
                        swift_type(element_type)
                    ),
                    ReadReturn::WithSize,
                ),
                VecLayout::Encoded => {
                    let (inner_reader, inner_return) = emit_read_expr(element, "pos", "$0");
                    let tuple_reader = match inner_return {
                        ReadReturn::BareValue(size) => format!("({}, {})", inner_reader, size),
                        ReadReturn::WithSize => inner_reader,
                    };
                    (
                        format!(
                            "wire.readArray(at: {}, reader: {{ {} }})",
                            offset_expr, tuple_reader
                        ),
                        ReadReturn::WithSize,
                    )
                }
            }
        }
        ReadOp::Result {
            tag_offset,
            ok,
            err,
        } => {
            let offset_expr = emit_offset_expr(tag_offset, base_name, base_expr);
            let (ok_reader, ok_return) = emit_read_expr(ok, "pos", "$0");
            let ok_tuple = match ok_return {
                ReadReturn::BareValue(size) => format!("({}, {})", ok_reader, size),
                ReadReturn::WithSize => ok_reader,
            };
            let (err_reader, err_return) = emit_read_expr(err, "pos", "$0");
            let err_tuple = match err_return {
                ReadReturn::BareValue(size) => format!("({}, {})", err_reader, size),
                ReadReturn::WithSize => err_reader,
            };
            (
                format!(
                    "wire.readResult(at: {}, okReader: {{ {} }}, errReader: {{ {} }})",
                    offset_expr, ok_tuple, err_tuple
                ),
                ReadReturn::WithSize,
            )
        }
        ReadOp::Record { id, offset, .. } => {
            let offset_expr = emit_offset_expr(offset, base_name, base_expr);
            (
                format!(
                    "{}.decode(wireBuffer: wire, at: {})",
                    pascal_case(id.as_str()),
                    offset_expr
                ),
                ReadReturn::WithSize,
            )
        }
        ReadOp::Enum { id, offset, layout } => {
            let offset_expr = emit_offset_expr(offset, base_name, base_expr);
            match layout {
                EnumLayout::CStyle { .. } => (
                    format!(
                        "{}(fromC: wire.readI32(at: {}))",
                        pascal_case(id.as_str()),
                        offset_expr
                    ),
                    ReadReturn::BareValue(4),
                ),
                EnumLayout::Data { .. } | EnumLayout::Recursive => (
                    format!(
                        "{}.decode(wireBuffer: wire, at: {})",
                        pascal_case(id.as_str()),
                        offset_expr
                    ),
                    ReadReturn::WithSize,
                ),
            }
        }
        ReadOp::Custom { underlying, .. } => emit_read_expr(underlying, base_name, base_expr),
    }
}

fn emit_write_data_op(op: &WriteOp) -> String {
    match op {
        WriteOp::Primitive { primitive, value } => {
            let v = render_value(value);
            emit_write_data_primitive(*primitive, &v)
        }
        WriteOp::String { value } => format!("data.appendString({})", render_value(value)),
        WriteOp::Bytes { value } => format!("data.appendBytes({})", render_value(value)),
        WriteOp::Builtin { id, value } => {
            let v = render_value(value);
            match id.as_str() {
                "Duration" => format!("data.appendDuration({})", v),
                "SystemTime" => format!("data.appendTimestamp({})", v),
                "Uuid" => format!("data.appendUuid({})", v),
                "Url" => format!("data.appendString({}.absoluteString)", v),
                _ => format!("{}.wireEncodeTo(&data)", v),
            }
        }
        WriteOp::Option { value, some } => {
            let inner = emit_write_data(some);
            format!(
                "if let v = {} {{ data.appendU8(1); {} }} else {{ data.appendU8(0) }}",
                render_value(value),
                inner
            )
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => {
            let v = render_value(value);
            if matches!(element_type, TypeExpr::Primitive(PrimitiveType::U8)) {
                return format!("data.appendBytes({})", v);
            }
            match layout {
                VecLayout::Blittable { .. } => format!("data.appendBlittableArray({})", v),
                VecLayout::Encoded => {
                    let inner = emit_write_data(element);
                    format!(
                        "data.appendU32(UInt32({}.count)); for item in {} {{ {} }}",
                        v, v, inner
                    )
                }
            }
        }
        WriteOp::Record { value, .. } => format!("{}.wireEncodeTo(&data)", render_value(value)),
        WriteOp::Enum { value, layout, .. } => {
            let v = render_value(value);
            match layout {
                EnumLayout::CStyle { .. } => format!("data.appendI32({}.rawValue)", v),
                EnumLayout::Data { .. } | EnumLayout::Recursive => {
                    format!("{}.wireEncodeTo(&data)", v)
                }
            }
        }
        WriteOp::Result { value, ok, err } => {
            let v = render_value(value);
            let ok_data = emit_write_data(ok);
            let err_data = emit_write_data(err);
            format!(
                "switch {} {{ case .success(let okVal): data.appendU8(0); {}; case .failure(let errVal): data.appendU8(1); {} }}",
                v, ok_data, err_data
            )
        }
        WriteOp::Custom { underlying, .. } => emit_write_data(underlying),
    }
}

fn emit_write_data_primitive(primitive: PrimitiveType, v: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("data.appendBool({})", v),
        PrimitiveType::I8 => format!("data.appendI8({})", v),
        PrimitiveType::U8 => format!("data.appendU8({})", v),
        PrimitiveType::I16 => format!("data.appendI16({})", v),
        PrimitiveType::U16 => format!("data.appendU16({})", v),
        PrimitiveType::I32 => format!("data.appendI32({})", v),
        PrimitiveType::U32 => format!("data.appendU32({})", v),
        PrimitiveType::I64 => format!("data.appendI64({})", v),
        PrimitiveType::U64 => format!("data.appendU64({})", v),
        PrimitiveType::ISize => format!("data.appendI64(Int64({}))", v),
        PrimitiveType::USize => format!("data.appendU64(UInt64({}))", v),
        PrimitiveType::F32 => format!("data.appendF32({})", v),
        PrimitiveType::F64 => format!("data.appendF64({})", v),
    }
}

fn emit_write_bytes_op(op: &WriteOp) -> String {
    match op {
        WriteOp::Primitive { primitive, value } => {
            let v = render_value(value);
            emit_write_bytes_primitive(*primitive, &v)
        }
        WriteOp::String { value } => format!("bytes.appendString({})", render_value(value)),
        WriteOp::Bytes { value } => format!("bytes.appendBytes({})", render_value(value)),
        WriteOp::Builtin { id, value } => {
            let v = render_value(value);
            match id.as_str() {
                "Duration" => format!("bytes.appendDuration({})", v),
                "SystemTime" => format!("bytes.appendTimestamp({})", v),
                "Uuid" => format!("bytes.appendUuid({})", v),
                "Url" => format!("bytes.appendString({}.absoluteString)", v),
                _ => format!("{}.wireEncodeToBytes(&bytes)", v),
            }
        }
        WriteOp::Option { value, some } => {
            let inner = emit_write_bytes(some);
            format!(
                "if let v = {} {{ bytes.appendU8(1); {} }} else {{ bytes.appendU8(0) }}",
                render_value(value),
                inner
            )
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => {
            let v = render_value(value);
            if matches!(element_type, TypeExpr::Primitive(PrimitiveType::U8)) {
                return format!("bytes.appendBytes({})", v);
            }
            match layout {
                VecLayout::Blittable { .. } => format!("bytes.appendBlittableArray({})", v),
                VecLayout::Encoded => {
                    let inner = emit_write_bytes(element);
                    format!(
                        "bytes.appendU32(UInt32({}.count)); for item in {} {{ {} }}",
                        v, v, inner
                    )
                }
            }
        }
        WriteOp::Record { value, .. } => {
            format!("{}.wireEncodeToBytes(&bytes)", render_value(value))
        }
        WriteOp::Enum { value, layout, .. } => {
            let v = render_value(value);
            match layout {
                EnumLayout::CStyle { .. } => format!("bytes.appendI32({}.rawValue)", v),
                EnumLayout::Data { .. } | EnumLayout::Recursive => {
                    format!("{}.wireEncodeToBytes(&bytes)", v)
                }
            }
        }
        WriteOp::Result { value, ok, err } => {
            let v = render_value(value);
            let ok_bytes = emit_write_bytes(ok);
            let err_bytes = emit_write_bytes(err);
            format!(
                "switch {} {{ case .success(let okVal): bytes.appendU8(0); {}; case .failure(let errVal): bytes.appendU8(1); {} }}",
                v, ok_bytes, err_bytes
            )
        }
        WriteOp::Custom { underlying, .. } => emit_write_bytes(underlying),
    }
}

fn emit_write_bytes_primitive(primitive: PrimitiveType, v: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("bytes.appendBool({})", v),
        PrimitiveType::I8 => format!("bytes.appendI8({})", v),
        PrimitiveType::U8 => format!("bytes.appendU8({})", v),
        PrimitiveType::I16 => format!("bytes.appendI16({})", v),
        PrimitiveType::U16 => format!("bytes.appendU16({})", v),
        PrimitiveType::I32 => format!("bytes.appendI32({})", v),
        PrimitiveType::U32 => format!("bytes.appendU32({})", v),
        PrimitiveType::I64 => format!("bytes.appendI64({})", v),
        PrimitiveType::U64 => format!("bytes.appendU64({})", v),
        PrimitiveType::ISize => format!("bytes.appendI64(Int64({}))", v),
        PrimitiveType::USize => format!("bytes.appendU64(UInt64({}))", v),
        PrimitiveType::F32 => format!("bytes.appendF32({})", v),
        PrimitiveType::F64 => format!("bytes.appendF64({})", v),
    }
}

fn emit_offset_expr(offset: &OffsetExpr, base_name: &str, base_expr: &str) -> String {
    match offset {
        OffsetExpr::Fixed(value) => value.to_string(),
        OffsetExpr::Base => base_expr.to_string(),
        OffsetExpr::BasePlus(add) => {
            if *add == 0 {
                base_expr.to_string()
            } else {
                format!("{} + {}", base_expr, add)
            }
        }
        OffsetExpr::Var(name) => {
            if name == base_name {
                base_expr.to_string()
            } else {
                name.to_string()
            }
        }
        OffsetExpr::VarPlus(name, add) => {
            let base = if name == base_name {
                base_expr.to_string()
            } else {
                name.to_string()
            };
            if *add == 0 {
                base
            } else {
                format!("{} + {}", base, add)
            }
        }
    }
}
