use riff_ffi_rules::naming::to_upper_camel_case as pascal_case;

use crate::ir::codec::{CodecPlan, EnumLayout, RecordLayout, VecLayout};
use crate::ir::types::PrimitiveType;

const OFFSET_VAR: &str = "pos";

pub fn swift_type(codec: &CodecPlan) -> String {
    match codec {
        CodecPlan::Void => "Void".to_string(),
        CodecPlan::Primitive(p) => swift_primitive(*p),
        CodecPlan::String => "String".to_string(),
        CodecPlan::Bytes => "Data".to_string(),
        CodecPlan::Builtin(id) => swift_builtin(id.as_str()),
        CodecPlan::Option(inner) => format!("{}?", swift_type(inner)),
        CodecPlan::Vec { element, .. } => {
            if matches!(element.as_ref(), CodecPlan::Primitive(PrimitiveType::U8)) {
                "Data".to_string()
            } else {
                format!("[{}]", swift_type(element))
            }
        }
        CodecPlan::Result { ok, err } => {
            format!("Result<{}, {}>", swift_type(ok), swift_type(err))
        }
        CodecPlan::Record { id, .. } => pascal_case(id.as_str()),
        CodecPlan::Enum { id, .. } => pascal_case(id.as_str()),
        CodecPlan::Custom { id, .. } => pascal_case(id.as_str()),
    }
}

pub fn swift_primitive(p: PrimitiveType) -> String {
    match p {
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

pub fn swift_builtin(id: &str) -> String {
    match id {
        "Duration" => "TimeInterval",
        "SystemTime" => "Date",
        "Uuid" => "UUID",
        "Url" => "URL",
        other => other,
    }
    .to_string()
}

pub fn decode_inline(codec: &CodecPlan) -> String {
    let (reader, decode_return) = decode_expr(codec);
    match decode_return {
        DecodeReturn::BareValue(size) => {
            format!("{{ let v = {}; {} += {}; return v }}()", reader, OFFSET_VAR, size)
        }
        DecodeReturn::WithSize => {
            format!("{{ let (v, s) = {}; {} += s; return v }}()", reader, OFFSET_VAR)
        }
    }
}

pub fn decode_stream_item(codec: &CodecPlan) -> String {
    let (reader, decode_return) = decode_expr(codec);
    let reader = reader.replace(OFFSET_VAR, "offset");
    match decode_return {
        DecodeReturn::BareValue(size) => {
            format!("{{ let v = {}; offset += {}; return v }}()", reader, size)
        }
        DecodeReturn::WithSize => {
            format!("{{ let (v, s) = {}; offset += s; return v }}()", reader)
        }
    }
}

pub fn decode_value_at(codec: &CodecPlan, offset_expr: &str) -> String {
    let (reader, decode_return) = decode_expr(codec);
    let expr = reader.replace(OFFSET_VAR, offset_expr);
    match decode_return {
        DecodeReturn::BareValue(_) => expr,
        DecodeReturn::WithSize => format!("{}.value", expr),
    }
}

pub fn decode_result_ok_throw(ok_codec: &CodecPlan, err_codec: &CodecPlan) -> String {
    let ok_decode = decode_value_at(ok_codec, "$0");
    let err_decode = match err_codec {
        CodecPlan::String => "FfiError(message: wire.readString(at: $0).value)".to_string(),
        _ => decode_value_at(err_codec, "$0"),
    };
    format!(
        "try wire.readResultOrThrow(at: 0, ok: {{ {} }}, err: {{ {} }})",
        ok_decode, err_decode
    )
}

pub fn decode_with_wire_buffer(codec: &CodecPlan, wire_buffer_expr: &str) -> String {
    let (reader, decode_return) = decode_expr(codec);
    let wire_let = format!("{{ let wire = {}; return wire }}()", wire_buffer_expr);
    let expr = if reader.contains("wireBuffer: wire") {
        reader
            .replace("wireBuffer: wire", &format!("wireBuffer: {}", wire_let))
            .replace(OFFSET_VAR, "0")
    } else {
        reader
            .replace("wire", &wire_let)
            .replace(OFFSET_VAR, "0")
    };
    match decode_return {
        DecodeReturn::BareValue(_) => expr,
        DecodeReturn::WithSize => format!("{}.value", expr),
    }
}

pub fn size_expr(codec: &CodecPlan, name: &str) -> String {
    encode_info(codec, name).0
}

pub fn encode_data(codec: &CodecPlan, name: &str) -> String {
    encode_info(codec, name).1
}

pub fn encode_bytes(codec: &CodecPlan, name: &str) -> String {
    encode_info(codec, name).2
}

pub fn decode_at_offset(codec: &CodecPlan, base: &str, offset: usize) -> String {
    match codec {
        CodecPlan::Primitive(p) => decode_primitive_at_offset(*p, base, offset),
        _ => panic!("decode_at_offset only supports primitives"),
    }
}

fn decode_primitive_at_offset(p: PrimitiveType, base: &str, offset: usize) -> String {
    let offset_expr = if offset == 0 {
        base.to_string()
    } else {
        format!("{} + {}", base, offset)
    };
    match p {
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
    }
}

pub fn encode_primitive_value(codec: &CodecPlan, name: &str) -> String {
    match codec {
        CodecPlan::Primitive(p) => encode_primitive_append(*p, name),
        _ => panic!("encode_primitive_value only supports primitives"),
    }
}

fn encode_primitive_append(p: PrimitiveType, name: &str) -> String {
    match p {
        PrimitiveType::Bool => format!("data.appendBool({})", name),
        PrimitiveType::I8 => format!("data.appendI8({})", name),
        PrimitiveType::U8 => format!("data.appendU8({})", name),
        PrimitiveType::I16 => format!("data.appendI16({})", name),
        PrimitiveType::U16 => format!("data.appendU16({})", name),
        PrimitiveType::I32 => format!("data.appendI32({})", name),
        PrimitiveType::U32 => format!("data.appendU32({})", name),
        PrimitiveType::I64 => format!("data.appendI64({})", name),
        PrimitiveType::U64 => format!("data.appendU64({})", name),
        PrimitiveType::ISize => format!("data.appendI64(Int64({}))", name),
        PrimitiveType::USize => format!("data.appendU64(UInt64({}))", name),
        PrimitiveType::F32 => format!("data.appendF32({})", name),
        PrimitiveType::F64 => format!("data.appendF64({})", name),
    }
}

enum DecodeReturn {
    BareValue(usize),
    WithSize,
}

fn decode_expr(codec: &CodecPlan) -> (String, DecodeReturn) {
    match codec {
        CodecPlan::Void => ("()".to_string(), DecodeReturn::BareValue(0)),
        CodecPlan::Primitive(p) => decode_primitive(*p),
        CodecPlan::String => (
            format!("wire.readString(at: {})", OFFSET_VAR),
            DecodeReturn::WithSize,
        ),
        CodecPlan::Bytes => (
            format!("wire.readBytesWithSize(at: {})", OFFSET_VAR),
            DecodeReturn::WithSize,
        ),
        CodecPlan::Builtin(id) => decode_builtin(id.as_str()),
        CodecPlan::Option(inner) => decode_option(inner),
        CodecPlan::Vec { element, layout } => decode_vec(element, layout),
        CodecPlan::Result { ok, err } => decode_result(ok, err),
        CodecPlan::Record { id, layout } => decode_record(id.as_str(), layout),
        CodecPlan::Enum { id, layout } => decode_enum(id.as_str(), layout),
        CodecPlan::Custom { underlying, .. } => decode_expr(underlying),
    }
}

fn decode_primitive(p: PrimitiveType) -> (String, DecodeReturn) {
    match p {
        PrimitiveType::Bool => (format!("wire.readBool(at: {})", OFFSET_VAR), DecodeReturn::BareValue(1)),
        PrimitiveType::I8 => (format!("wire.readI8(at: {})", OFFSET_VAR), DecodeReturn::BareValue(1)),
        PrimitiveType::U8 => (format!("wire.readU8(at: {})", OFFSET_VAR), DecodeReturn::BareValue(1)),
        PrimitiveType::I16 => (format!("wire.readI16(at: {})", OFFSET_VAR), DecodeReturn::BareValue(2)),
        PrimitiveType::U16 => (format!("wire.readU16(at: {})", OFFSET_VAR), DecodeReturn::BareValue(2)),
        PrimitiveType::I32 => (format!("wire.readI32(at: {})", OFFSET_VAR), DecodeReturn::BareValue(4)),
        PrimitiveType::U32 => (format!("wire.readU32(at: {})", OFFSET_VAR), DecodeReturn::BareValue(4)),
        PrimitiveType::I64 => (format!("wire.readI64(at: {})", OFFSET_VAR), DecodeReturn::BareValue(8)),
        PrimitiveType::U64 => (format!("wire.readU64(at: {})", OFFSET_VAR), DecodeReturn::BareValue(8)),
        PrimitiveType::ISize => (format!("Int(wire.readI64(at: {}))", OFFSET_VAR), DecodeReturn::BareValue(8)),
        PrimitiveType::USize => (format!("UInt(wire.readU64(at: {}))", OFFSET_VAR), DecodeReturn::BareValue(8)),
        PrimitiveType::F32 => (format!("wire.readF32(at: {})", OFFSET_VAR), DecodeReturn::BareValue(4)),
        PrimitiveType::F64 => (format!("wire.readF64(at: {})", OFFSET_VAR), DecodeReturn::BareValue(8)),
    }
}

fn decode_builtin(id: &str) -> (String, DecodeReturn) {
    match id {
        "Duration" => (
            format!("wire.readDuration(at: {})", OFFSET_VAR),
            DecodeReturn::BareValue(12),
        ),
        "SystemTime" => (
            format!("wire.readTimestamp(at: {})", OFFSET_VAR),
            DecodeReturn::BareValue(12),
        ),
        "Uuid" => (
            format!("wire.readUuid(at: {})", OFFSET_VAR),
            DecodeReturn::BareValue(16),
        ),
        "Url" => (
            format!("wire.readUrl(at: {})", OFFSET_VAR),
            DecodeReturn::WithSize,
        ),
        _ => (
            format!("wire.read{}(at: {})", pascal_case(id), OFFSET_VAR),
            DecodeReturn::WithSize,
        ),
    }
}

fn decode_record(name: &str, layout: &RecordLayout) -> (String, DecodeReturn) {
    let class_name = pascal_case(name);
    match layout {
        RecordLayout::Blittable { .. } | RecordLayout::Encoded { .. } | RecordLayout::Recursive => (
            format!("{}.decode(wireBuffer: wire, at: {})", class_name, OFFSET_VAR),
            DecodeReturn::WithSize,
        ),
    }
}

fn decode_enum(name: &str, layout: &EnumLayout) -> (String, DecodeReturn) {
    let class_name = pascal_case(name);
    match layout {
        EnumLayout::CStyle { .. } => (
            format!("{}(fromC: wire.readI32(at: {}))", class_name, OFFSET_VAR),
            DecodeReturn::BareValue(4),
        ),
        EnumLayout::Data { .. } | EnumLayout::Recursive => (
            format!("{}.decode(wireBuffer: wire, at: {})", class_name, OFFSET_VAR),
            DecodeReturn::WithSize,
        ),
    }
}

fn decode_vec(element: &CodecPlan, layout: &VecLayout) -> (String, DecodeReturn) {
    if matches!(element, CodecPlan::Primitive(PrimitiveType::U8)) {
        return (
            format!("wire.readBytesWithSize(at: {})", OFFSET_VAR),
            DecodeReturn::WithSize,
        );
    }

    match layout {
        VecLayout::Blittable { .. } => {
            let element_type = swift_type(element);
            (
                format!("wire.readBlittableArrayWithSize(at: {}, as: {}.self)", OFFSET_VAR, element_type),
                DecodeReturn::WithSize,
            )
        }
        VecLayout::Encoded => {
            let (inner_reader, inner_return) = decode_expr(element);
            let inner_replaced = inner_reader.replace(OFFSET_VAR, "$0");
            let tuple_reader = match inner_return {
                DecodeReturn::BareValue(size) => {
                    format!("({}, {})", inner_replaced, size)
                }
                DecodeReturn::WithSize => inner_replaced,
            };
            (
                format!("wire.readArray(at: {}, reader: {{ {} }})", OFFSET_VAR, tuple_reader),
                DecodeReturn::WithSize,
            )
        }
    }
}

fn decode_option(inner: &CodecPlan) -> (String, DecodeReturn) {
    let (inner_reader, inner_return) = decode_expr(inner);
    let inner_replaced = inner_reader.replace(OFFSET_VAR, "$0");
    let tuple_reader = match inner_return {
        DecodeReturn::BareValue(size) => {
            format!("({}, {})", inner_replaced, size)
        }
        DecodeReturn::WithSize => inner_replaced,
    };
    (
        format!("wire.readOptional(at: {}, reader: {{ {} }})", OFFSET_VAR, tuple_reader),
        DecodeReturn::WithSize,
    )
}

fn decode_result(ok: &CodecPlan, err: &CodecPlan) -> (String, DecodeReturn) {
    let (ok_reader, ok_return) = decode_expr(ok);
    let (err_reader, err_return) = decode_expr(err);
    
    let ok_replaced = ok_reader.replace(OFFSET_VAR, "$0");
    let ok_tuple = match ok_return {
        DecodeReturn::BareValue(size) => {
            format!("({}, {})", ok_replaced, size)
        }
        DecodeReturn::WithSize => ok_replaced,
    };
    
    let err_replaced = err_reader.replace(OFFSET_VAR, "$0");
    let err_tuple = match err_return {
        DecodeReturn::BareValue(size) => {
            format!("({}, {})", err_replaced, size)
        }
        DecodeReturn::WithSize => err_replaced,
    };
    
    (
        format!("wire.readResult(at: {}, okReader: {{ {} }}, errReader: {{ {} }})", OFFSET_VAR, ok_tuple, err_tuple),
        DecodeReturn::WithSize,
    )
}

fn encode_info(codec: &CodecPlan, name: &str) -> (String, String, String) {
    match codec {
        CodecPlan::Void => ("0".to_string(), String::new(), String::new()),
        CodecPlan::Primitive(p) => encode_primitive(*p, name),
        CodecPlan::String => (
            format!("(4 + {}.utf8.count)", name),
            format!("data.appendString({})", name),
            format!("bytes.appendString({})", name),
        ),
        CodecPlan::Bytes => (
            format!("(4 + {}.count)", name),
            format!("data.appendBytes({})", name),
            format!("bytes.appendBytes({})", name),
        ),
        CodecPlan::Builtin(id) => encode_builtin(id.as_str(), name),
        CodecPlan::Option(inner) => encode_option(inner, name),
        CodecPlan::Vec { element, layout } => encode_vec(element, layout, name),
        CodecPlan::Result { ok, err } => encode_result(ok, err, name),
        CodecPlan::Record { layout, .. } => encode_record(layout, name),
        CodecPlan::Enum { layout, .. } => encode_enum(layout, name),
        CodecPlan::Custom { underlying, .. } => encode_info(underlying, name),
    }
}

fn encode_primitive(p: PrimitiveType, name: &str) -> (String, String, String) {
    match p {
        PrimitiveType::Bool => ("1".into(), format!("data.appendBool({})", name), format!("bytes.appendBool({})", name)),
        PrimitiveType::I8 => ("1".into(), format!("data.appendI8({})", name), format!("bytes.appendI8({})", name)),
        PrimitiveType::U8 => ("1".into(), format!("data.appendU8({})", name), format!("bytes.appendU8({})", name)),
        PrimitiveType::I16 => ("2".into(), format!("data.appendI16({})", name), format!("bytes.appendI16({})", name)),
        PrimitiveType::U16 => ("2".into(), format!("data.appendU16({})", name), format!("bytes.appendU16({})", name)),
        PrimitiveType::I32 => ("4".into(), format!("data.appendI32({})", name), format!("bytes.appendI32({})", name)),
        PrimitiveType::U32 => ("4".into(), format!("data.appendU32({})", name), format!("bytes.appendU32({})", name)),
        PrimitiveType::I64 => ("8".into(), format!("data.appendI64({})", name), format!("bytes.appendI64({})", name)),
        PrimitiveType::U64 => ("8".into(), format!("data.appendU64({})", name), format!("bytes.appendU64({})", name)),
        PrimitiveType::ISize => ("8".into(), format!("data.appendI64(Int64({}))", name), format!("bytes.appendI64(Int64({}))", name)),
        PrimitiveType::USize => ("8".into(), format!("data.appendU64(UInt64({}))", name), format!("bytes.appendU64(UInt64({}))", name)),
        PrimitiveType::F32 => ("4".into(), format!("data.appendF32({})", name), format!("bytes.appendF32({})", name)),
        PrimitiveType::F64 => ("8".into(), format!("data.appendF64({})", name), format!("bytes.appendF64({})", name)),
    }
}

fn encode_builtin(id: &str, name: &str) -> (String, String, String) {
    match id {
        "Duration" => (
            "12".to_string(),
            format!("data.appendDuration({})", name),
            format!("bytes.appendDuration({})", name),
        ),
        "SystemTime" => (
            "12".to_string(),
            format!("data.appendTimestamp({})", name),
            format!("bytes.appendTimestamp({})", name),
        ),
        "Uuid" => (
            "16".to_string(),
            format!("data.appendUuid({})", name),
            format!("bytes.appendUuid({})", name),
        ),
        "Url" => (
            format!("(4 + {}.absoluteString.utf8.count)", name),
            format!("data.appendString({}.absoluteString)", name),
            format!("bytes.appendString({}.absoluteString)", name),
        ),
        _ => (
            format!("{}.wireEncodedSize()", name),
            format!("{}.wireEncodeTo(&data)", name),
            format!("{}.wireEncodeToBytes(&bytes)", name),
        ),
    }
}

fn encode_record(layout: &RecordLayout, name: &str) -> (String, String, String) {
    match layout {
        RecordLayout::Blittable { size, .. } => (
            size.to_string(),
            format!("{}.wireEncodeTo(&data)", name),
            format!("{}.wireEncodeToBytes(&bytes)", name),
        ),
        RecordLayout::Encoded { .. } | RecordLayout::Recursive => (
            format!("{}.wireEncodedSize()", name),
            format!("{}.wireEncodeTo(&data)", name),
            format!("{}.wireEncodeToBytes(&bytes)", name),
        ),
    }
}

fn encode_enum(layout: &EnumLayout, name: &str) -> (String, String, String) {
    match layout {
        EnumLayout::CStyle { .. } => (
            "4".to_string(),
            format!("data.appendI32({}.rawValue)", name),
            format!("bytes.appendI32({}.rawValue)", name),
        ),
        EnumLayout::Data { .. } | EnumLayout::Recursive => (
            format!("{}.wireEncodedSize()", name),
            format!("{}.wireEncodeTo(&data)", name),
            format!("{}.wireEncodeToBytes(&bytes)", name),
        ),
    }
}

fn encode_vec(element: &CodecPlan, layout: &VecLayout, name: &str) -> (String, String, String) {
    if matches!(element, CodecPlan::Primitive(PrimitiveType::U8)) {
        return (
            format!("(4 + {}.count)", name),
            format!("data.appendBytes({})", name),
            format!("bytes.appendBytes({})", name),
        );
    }

    let (inner_size, inner_data, inner_bytes) = encode_info(element, "item");

    match layout {
        VecLayout::Blittable { element_size } => (
            format!("(4 + {}.count * {})", name, element_size),
            format!("data.appendBlittableArray({})", name),
            format!("bytes.appendBlittableArray({})", name),
        ),
        VecLayout::Encoded => (
            format!("(4 + {}.reduce(0) {{ $0 + {} }})", name, inner_size.replace("item", "$1")),
            format!("data.appendU32(UInt32({}.count)); for item in {} {{ {} }}", name, name, inner_data),
            format!("bytes.appendU32(UInt32({}.count)); for item in {} {{ {} }}", name, name, inner_bytes),
        ),
    }
}

fn encode_option(inner: &CodecPlan, name: &str) -> (String, String, String) {
    let (inner_size, inner_data, inner_bytes) = encode_info(inner, "v");
    (
        format!("({}.map {{ v in 1 + {} }} ?? 1)", name, inner_size),
        format!("if let v = {} {{ data.appendU8(1); {} }} else {{ data.appendU8(0) }}", name, inner_data),
        format!("if let v = {} {{ bytes.appendU8(1); {} }} else {{ bytes.appendU8(0) }}", name, inner_bytes),
    )
}

fn encode_result(ok: &CodecPlan, err: &CodecPlan, name: &str) -> (String, String, String) {
    let (ok_size, ok_data, ok_bytes) = encode_info(ok, "okVal");
    let (err_size, err_data, err_bytes) = encode_info(err, "errVal");
    let ok_size_fixed = ok_size.chars().all(|c| c.is_ascii_digit());
    let err_size_fixed = err_size.chars().all(|c| c.is_ascii_digit());
    let ok_size_binding = if ok_size_fixed { "_" } else { "let okVal" };
    let err_size_binding = if err_size_fixed { "_" } else { "let errVal" };
    (
        format!(
            "({{ switch {} {{ case .success({}): return 1 + {}; case .failure({}): return 1 + {} }} }}())",
            name, ok_size_binding, ok_size, err_size_binding, err_size
        ),
        format!(
            "switch {} {{ case .success(let okVal): data.appendU8(0); {}; case .failure(let errVal): data.appendU8(1); {} }}",
            name, ok_data, err_data
        ),
        format!(
            "switch {} {{ case .success(let okVal): bytes.appendU8(0); {}; case .failure(let errVal): bytes.appendU8(1); {} }}",
            name, ok_bytes, err_bytes
        ),
    )
}
