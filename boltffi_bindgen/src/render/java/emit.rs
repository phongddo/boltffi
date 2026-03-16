use crate::ir::abi::AbiContract;
use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::contract::FfiContract;
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::JavaOptions;
use super::lower::JavaLowerer;
use super::mappings;
use super::names::NamingConvention;
use super::plan::JavaEnumKind;
use super::templates::{
    CStyleEnumTemplate, DataEnumAbstractTemplate, DataEnumSealedTemplate, FunctionsTemplate,
    NativeTemplate, PreambleTemplate, RecordTemplate,
};
use askama::Template;

pub struct JavaFile {
    pub file_name: String,
    pub source: String,
}

pub struct JavaOutput {
    pub files: Vec<JavaFile>,
    pub class_name: String,
    pub package_path: String,
}

pub struct JavaEmitter;

impl JavaEmitter {
    pub fn emit(
        ffi: &FfiContract,
        abi: &AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> JavaOutput {
        let lowerer = JavaLowerer::new(ffi, abi, package_name, module_name, options);
        let module = lowerer.module();
        let package_path = module.package_path();
        let class_name = module.class_name.clone();

        let mut files = Vec::new();

        for enumeration in &module.enums {
            let source = match enumeration.kind {
                JavaEnumKind::CStyle => {
                    let template = CStyleEnumTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("c-style enum template failed")
                }
                JavaEnumKind::SealedInterface => {
                    let template = DataEnumSealedTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("sealed enum template failed")
                }
                JavaEnumKind::AbstractClass => {
                    let template = DataEnumAbstractTemplate {
                        enumeration,
                        package_name: &module.package_name,
                    };
                    template.render().expect("abstract enum template failed")
                }
            };
            files.push(JavaFile {
                file_name: format!("{}.java", enumeration.class_name),
                source,
            });
        }

        for record in &module.records {
            let record_template = RecordTemplate {
                record,
                package_name: &module.package_name,
            };
            files.push(JavaFile {
                file_name: format!("{}.java", record.class_name),
                source: record_template.render().expect("record template failed"),
            });
        }

        let mut main_source = String::new();

        let preamble = PreambleTemplate { module: &module };
        main_source.push_str(&preamble.render().expect("preamble template failed"));

        let native = NativeTemplate { module: &module };
        main_source.push_str(&native.render().expect("native template failed"));

        main_source.push('\n');

        let functions = FunctionsTemplate { module: &module };
        main_source.push_str(&functions.render().expect("functions template failed"));

        files.push(JavaFile {
            file_name: format!("{}.java", class_name),
            source: main_source,
        });

        JavaOutput {
            files,
            class_name,
            package_path,
        }
    }
}

fn render_value(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::Instance => String::new(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => NamingConvention::field_name(name),
        ValueExpr::Field(parent, field) => {
            let parent_str = render_value(parent);
            let field_str = NamingConvention::field_name(field.as_str());
            if parent_str.is_empty() {
                field_str
            } else {
                format!("{}.{}", parent_str, field_str)
            }
        }
    }
}

#[derive(Default)]
struct JavaEmitContext {
    read_lambda_index: usize,
    write_loop_index: usize,
    size_lambda_index: usize,
}

impl JavaEmitContext {
    fn next_read_lambda(&mut self) -> String {
        let next_index = self.read_lambda_index;
        self.read_lambda_index += 1;
        format!("readIndex{}", next_index)
    }

    fn next_write_loop_var(&mut self) -> String {
        let next_index = self.write_loop_index;
        self.write_loop_index += 1;
        format!("item{}", next_index)
    }

    fn next_size_lambda(&mut self) -> String {
        let next_index = self.size_lambda_index;
        self.size_lambda_index += 1;
        format!("sizeItem{}", next_index)
    }
}

pub fn primitive_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "readBool",
        PrimitiveType::I8 | PrimitiveType::U8 => "readI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "readI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "readI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readI64"
        }
        PrimitiveType::F32 => "readF32",
        PrimitiveType::F64 => "readF64",
    }
}

fn primitive_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "writeBool",
        PrimitiveType::I8 | PrimitiveType::U8 => "writeI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "writeI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "writeI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "writeI64"
        }
        PrimitiveType::F32 => "writeF32",
        PrimitiveType::F64 => "writeF64",
    }
}

pub fn emit_reader_read(seq: &ReadSeq) -> String {
    let mut context = JavaEmitContext::default();
    emit_reader_read_with_context(seq, &mut context)
}

fn emit_reader_read_with_context(seq: &ReadSeq, context: &mut JavaEmitContext) -> String {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => {
            format!("reader.{}()", primitive_read_method(*primitive))
        }
        ReadOp::String { .. } => "reader.readString()".to_string(),
        ReadOp::Bytes { .. } => "reader.readBytes()".to_string(),
        ReadOp::Record { id, .. } => {
            format!(
                "{}.decode(reader)",
                NamingConvention::class_name(id.as_str())
            )
        }
        ReadOp::Enum { id, layout, .. } => match layout {
            EnumLayout::CStyle { tag_type, .. } => {
                format!(
                    "{}.fromValue(reader.{}())",
                    NamingConvention::class_name(id.as_str()),
                    primitive_read_method(*tag_type),
                )
            }
            EnumLayout::Data { .. } | EnumLayout::Recursive => {
                format!(
                    "{}.decode(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
        },
        ReadOp::Option { some, .. } => {
            let inner = emit_reader_read_with_context(some, context);
            format!(
                "reader.readI8() == 0 ? java.util.Optional.empty() : java.util.Optional.ofNullable({})",
                inner,
            )
        }
        ReadOp::Vec {
            element_type,
            element,
            layout,
            ..
        } => emit_reader_vec_with_context(element_type, element, layout, context),
        other => panic!("unsupported Java read op: {:?}", other),
    }
}

fn emit_reader_vec_with_context(
    element_type: &TypeExpr,
    element: &ReadSeq,
    layout: &VecLayout,
    context: &mut JavaEmitContext,
) -> String {
    match layout {
        VecLayout::Blittable { .. } => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!("reader.{}()", primitive_array_read_method(*primitive))
            }
            TypeExpr::Record(id) => {
                format!(
                    "{}.decodeBlittableVec(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            _ => {
                let inner = emit_reader_read_with_context(element, context);
                let lambda_var = context.next_read_lambda();
                format!("reader.readList({} -> {})", lambda_var, inner)
            }
        },
        VecLayout::Encoded => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!("reader.{}()", primitive_array_read_method(*primitive))
            }
            _ => {
                let inner = emit_reader_read_with_context(element, context);
                let lambda_var = context.next_read_lambda();
                format!("reader.readList({} -> {})", lambda_var, inner)
            }
        },
    }
}

pub fn primitive_array_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "readBooleanArray",
        PrimitiveType::I8 | PrimitiveType::U8 => "readByteArray",
        PrimitiveType::I16 | PrimitiveType::U16 => "readShortArray",
        PrimitiveType::I32 | PrimitiveType::U32 => "readIntArray",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readLongArray"
        }
        PrimitiveType::F32 => "readFloatArray",
        PrimitiveType::F64 => "readDoubleArray",
    }
}

pub fn emit_write_expr(seq: &WriteSeq, writer_name: &str) -> String {
    let mut context = JavaEmitContext::default();
    emit_write_expr_with_context(seq, writer_name, &mut context)
}

fn emit_write_expr_with_context(
    seq: &WriteSeq,
    writer_name: &str,
    context: &mut JavaEmitContext,
) -> String {
    let op = seq.ops.first().expect("write ops");
    match op {
        WriteOp::Primitive { primitive, value } => {
            format!(
                "{}.{}({})",
                writer_name,
                primitive_write_method(*primitive),
                render_value(value)
            )
        }
        WriteOp::String { value } => {
            format!("{}.writeString({})", writer_name, render_value(value))
        }
        WriteOp::Bytes { value } => {
            format!("{}.writeBytes({})", writer_name, render_value(value))
        }
        WriteOp::Option { value, some } => {
            let option_expr = render_value(value);
            let inner = emit_write_expr_with_context(some, writer_name, context);
            let some_value_expr = format!("({}).get()", option_expr);
            let remapped_inner = replace_identifier_occurrences(&inner, "v", &some_value_expr);
            format!(
                "if (({}).isPresent()) {{ {}.writeI8((byte)1); {}; }} else {{ {}.writeI8((byte)0); }}",
                option_expr, writer_name, remapped_inner, writer_name,
            )
        }
        WriteOp::Record { value, .. } => {
            format!("{}.wireEncodeTo({})", render_value(value), writer_name)
        }
        WriteOp::Enum { value, layout, .. } => match layout {
            EnumLayout::CStyle { tag_type, .. } => {
                format!(
                    "{}.{}({}.value)",
                    writer_name,
                    primitive_write_method(*tag_type),
                    render_value(value),
                )
            }
            EnumLayout::Data { .. } | EnumLayout::Recursive => {
                format!("{}.wireEncodeTo({})", render_value(value), writer_name)
            }
        },
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => emit_write_vec_with_context(
            writer_name,
            &render_value(value),
            element_type,
            element,
            layout,
            context,
        ),
        other => panic!("unsupported Java write op: {:?}", other),
    }
}

fn emit_write_vec_with_context(
    writer_name: &str,
    value: &str,
    element_type: &TypeExpr,
    element: &WriteSeq,
    layout: &VecLayout,
    context: &mut JavaEmitContext,
) -> String {
    match layout {
        VecLayout::Blittable { .. } => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!(
                    "{}.{}({})",
                    writer_name,
                    primitive_array_write_method(*primitive),
                    value,
                )
            }
            TypeExpr::Record(id) => {
                format!(
                    "{}.encodeBlittableVec({}, {})",
                    NamingConvention::class_name(id.as_str()),
                    writer_name,
                    value,
                )
            }
            _ => {
                let inner = emit_write_expr_with_context(element, writer_name, context);
                let loop_var = context.next_write_loop_var();
                let remapped_inner = replace_identifier_occurrences(&inner, "item", &loop_var);
                let iter_type = java_type_for_iteration(element_type);
                format!(
                    "{}.writeI32({}.size()); for ({} {} : {}) {{ {}; }}",
                    writer_name, value, iter_type, loop_var, value, remapped_inner,
                )
            }
        },
        VecLayout::Encoded => match element_type {
            TypeExpr::Primitive(primitive) => {
                format!(
                    "{}.{}({})",
                    writer_name,
                    primitive_array_write_method(*primitive),
                    value,
                )
            }
            _ => {
                let inner = emit_write_expr_with_context(element, writer_name, context);
                let loop_var = context.next_write_loop_var();
                let remapped_inner = replace_identifier_occurrences(&inner, "item", &loop_var);
                let iter_type = java_type_for_iteration(element_type);
                format!(
                    "{}.writeI32({}.size()); for ({} {} : {}) {{ {}; }}",
                    writer_name, value, iter_type, loop_var, value, remapped_inner,
                )
            }
        },
    }
}

fn java_type_for_iteration(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Primitive(primitive) => mappings::java_boxed_type(*primitive).to_string(),
        TypeExpr::String => "String".to_string(),
        TypeExpr::Bytes => "byte[]".to_string(),
        TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
        TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
        TypeExpr::Option(inner) => {
            format!("java.util.Optional<{}>", java_type_for_iteration(inner))
        }
        TypeExpr::Vec(inner) => match inner.as_ref() {
            TypeExpr::Primitive(primitive) => {
                mappings::java_primitive_array_type(*primitive).to_string()
            }
            _ => format!("java.util.List<{}>", java_type_for_iteration(inner)),
        },
        _ => "Object".to_string(),
    }
}

fn primitive_array_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "writeBooleanArray",
        PrimitiveType::I8 | PrimitiveType::U8 => "writeByteArray",
        PrimitiveType::I16 | PrimitiveType::U16 => "writeShortArray",
        PrimitiveType::I32 | PrimitiveType::U32 => "writeIntArray",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "writeLongArray"
        }
        PrimitiveType::F32 => "writeFloatArray",
        PrimitiveType::F64 => "writeDoubleArray",
    }
}

pub fn emit_size_expr(size: &SizeExpr) -> String {
    let mut context = JavaEmitContext::default();
    emit_size_expr_with_context(size, &mut context)
}

fn emit_size_expr_with_context(size: &SizeExpr, context: &mut JavaEmitContext) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::StringLen(value) => {
            format!("(4 + ({}).length() * 3)", render_value(value),)
        }
        SizeExpr::BytesLen(value) => {
            format!("{}.length", render_value(value))
        }
        SizeExpr::WireSize { value, .. } => {
            format!("{}.wireEncodedSize()", render_value(value))
        }
        SizeExpr::OptionSize { value, inner } => {
            let option_expr = render_value(value);
            let inner_expr = emit_size_expr_with_context(inner, context);
            let some_value_expr = format!("({}).get()", option_expr);
            let remapped_inner = replace_identifier_occurrences(&inner_expr, "v", &some_value_expr);
            format!(
                "(1 + (({}).isPresent() ? ({}) : 0))",
                option_expr, remapped_inner,
            )
        }
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => emit_vec_size_expr_with_context(&render_value(value), inner, layout, context),
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(|part| emit_size_expr_with_context(part, context))
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({})", rendered)
        }
        other => panic!("unsupported Java size expr: {:?}", other),
    }
}

fn emit_vec_size_expr_with_context(
    value: &str,
    inner: &SizeExpr,
    layout: &VecLayout,
    context: &mut JavaEmitContext,
) -> String {
    match layout {
        VecLayout::Blittable { element_size } => {
            format!("(4 + {} * {})", emit_vec_length_expr(value), element_size)
        }
        VecLayout::Encoded => {
            let inner_expr = emit_size_expr_with_context(inner, context);
            if inner_expr.contains("item") {
                let lambda_var = context.next_size_lambda();
                let remapped_inner =
                    replace_identifier_occurrences(&inner_expr, "item", &lambda_var);
                format!(
                    "WireWriter.listWireSize({}, {} -> {})",
                    value, lambda_var, remapped_inner,
                )
            } else {
                format!("(4 + {} * {})", emit_vec_length_expr(value), inner_expr)
            }
        }
    }
}

fn emit_vec_length_expr(value: &str) -> String {
    format!("WireWriter.vecLength({})", value)
}

fn replace_identifier_occurrences(expression: &str, identifier: &str, replacement: &str) -> String {
    if identifier.is_empty() {
        return expression.to_string();
    }

    let mut result = String::with_capacity(expression.len());
    let mut cursor = 0;

    while let Some(relative_index) = expression[cursor..].find(identifier) {
        let start = cursor + relative_index;
        let end = start + identifier.len();
        let previous = expression[..start].chars().next_back();
        let next = expression[end..].chars().next();
        let previous_is_identifier = previous.map(is_identifier_char).unwrap_or(false);
        let next_is_identifier = next.map(is_identifier_char).unwrap_or(false);

        if previous_is_identifier || next_is_identifier {
            result.push_str(&expression[cursor..end]);
            cursor = end;
        } else {
            result.push_str(&expression[cursor..start]);
            result.push_str(replacement);
            cursor = end;
        }
    }

    result.push_str(&expression[cursor..]);
    result
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}
