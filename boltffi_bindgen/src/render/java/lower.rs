use std::collections::HashSet;

use super::JavaOptions;
use super::mappings;
use super::names::NamingConvention;
use super::plan::{
    JavaBlittableField, JavaBlittableLayout, JavaEnum, JavaEnumField, JavaEnumKind,
    JavaEnumVariant, JavaFunction, JavaModule, JavaParam, JavaRecord, JavaRecordField,
    JavaRecordShape, JavaReturnStrategy, JavaWireWriter,
};
use crate::ir::abi::{
    AbiCall, AbiContract, AbiEnum, AbiEnumField, AbiEnumPayload, AbiEnumVariant, AbiParam,
    AbiRecord, CallId, ParamRole,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{EnumDef, EnumRepr, FieldDef, FunctionDef, RecordDef, ReturnDef};
use crate::ir::ids::{FieldName, RecordId};
use crate::ir::ops::{
    FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq,
};
use crate::ir::plan::{ScalarOrigin, SpanContent, Transport};
use crate::ir::types::{PrimitiveType, TypeExpr};

pub struct JavaLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: JavaOptions,
    supported_types: HashSet<String>,
}

impl<'a> JavaLowerer<'a> {
    pub fn new(
        ffi: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> Self {
        let supported_types = Self::compute_supported_types(ffi, abi);
        Self {
            ffi,
            abi,
            package_name,
            module_name,
            options,
            supported_types,
        }
    }

    fn is_leaf_supported(ty: &TypeExpr, supported: &HashSet<String>) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
            TypeExpr::Record(id) => supported.contains(id.as_str()),
            TypeExpr::Enum(id) => supported.contains(id.as_str()),
            TypeExpr::Option(inner) => Self::is_leaf_supported(inner, supported),
            TypeExpr::Vec(inner) => Self::is_leaf_supported(inner, supported),
            _ => false,
        }
    }

    fn compute_supported_types(ffi: &FfiContract, abi: &AbiContract) -> HashSet<String> {
        let mut supported = HashSet::new();
        let mut changed = true;

        while changed {
            changed = false;

            for record in ffi.catalog.all_records() {
                let id = record.id.as_str();
                if supported.contains(id) {
                    continue;
                }
                let all_ok = record
                    .fields
                    .iter()
                    .all(|f| Self::is_leaf_supported(&f.type_expr, &supported));
                if all_ok {
                    supported.insert(id.to_string());
                    changed = true;
                }
            }

            for enumeration in ffi.catalog.all_enums() {
                let id = enumeration.id.as_str();
                if supported.contains(id) {
                    continue;
                }
                let abi_enum = abi
                    .enums
                    .iter()
                    .find(|ae| ae.id == enumeration.id)
                    .expect("abi enum missing");

                let all_ok = if abi_enum.is_c_style {
                    true
                } else {
                    abi_enum
                        .variants
                        .iter()
                        .all(|variant| match &variant.payload {
                            AbiEnumPayload::Unit => true,
                            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                                fields
                                    .iter()
                                    .all(|f| Self::is_leaf_supported(&f.type_expr, &supported))
                            }
                        })
                };

                if all_ok {
                    supported.insert(id.to_string());
                    changed = true;
                }
            }
        }

        supported
    }

    pub fn module(&self) -> JavaModule {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| self.module_name.clone())
            .replace('-', "_");

        let prefix = boltffi_ffi_rules::naming::ffi_prefix().to_string();

        let enums: Vec<JavaEnum> = self
            .ffi
            .catalog
            .all_enums()
            .filter(|e| self.supported_types.contains(e.id.as_str()))
            .map(|e| self.lower_enum(e))
            .collect();

        let records: Vec<JavaRecord> = self
            .ffi
            .catalog
            .all_records()
            .filter(|r| self.supported_types.contains(r.id.as_str()))
            .map(|r| self.lower_record(r))
            .collect();

        let functions: Vec<JavaFunction> = self
            .ffi
            .functions
            .iter()
            .filter(|f| !f.is_async && self.is_supported_function(f))
            .map(|f| self.lower_function(f))
            .collect();

        JavaModule {
            package_name: self.package_name.clone(),
            class_name: NamingConvention::class_name(&self.module_name),
            lib_name,
            java_version: self.options.min_java_version,
            prefix,
            records,
            enums,
            functions,
        }
    }

    fn is_supported_function(&self, func: &FunctionDef) -> bool {
        let params_ok = func
            .params
            .iter()
            .all(|p| self.is_supported_type(&p.type_expr));
        let return_ok = match &func.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_type(ty),
            ReturnDef::Result { .. } => false,
        };
        params_ok && return_ok
    }

    fn is_supported_type(&self, ty: &TypeExpr) -> bool {
        Self::is_leaf_supported(ty, &self.supported_types)
    }

    fn lower_record(&self, record: &RecordDef) -> JavaRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(&record.id, field))
            .collect();
        let blittable_layout = self.lower_blittable_layout(&record.id);
        let shape = if self.can_use_native_record_syntax(record) {
            JavaRecordShape::NativeRecord
        } else {
            JavaRecordShape::ClassicClass
        };
        JavaRecord {
            shape,
            class_name,
            fields,
            blittable_layout,
        }
    }

    fn can_use_native_record_syntax(&self, record: &RecordDef) -> bool {
        self.options.min_java_version.supports_records()
            && !record
                .fields
                .iter()
                .any(|field| Self::contains_primitive_array_component(&field.type_expr))
    }

    fn contains_primitive_array_component(ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Vec(inner) => {
                matches!(inner.as_ref(), TypeExpr::Primitive(_))
                    || Self::contains_primitive_array_component(inner)
            }
            TypeExpr::Option(inner) => Self::contains_primitive_array_component(inner),
            TypeExpr::Result { ok, err } => {
                Self::contains_primitive_array_component(ok)
                    || Self::contains_primitive_array_component(err)
            }
            _ => false,
        }
    }

    fn lower_blittable_layout(&self, record_id: &RecordId) -> Option<JavaBlittableLayout> {
        let abi_record = self.abi_record_for(record_id)?;
        if !abi_record.is_blittable {
            return None;
        }
        let struct_size = abi_record.size?;
        let fields = match abi_record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(Self::lower_blittable_field)
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        Some(JavaBlittableLayout {
            struct_size,
            fields,
        })
    }

    fn lower_blittable_field(field: &FieldReadOp) -> Option<JavaBlittableField> {
        let (primitive, offset) = match field.seq.ops.first() {
            Some(ReadOp::Primitive { primitive, offset }) => (*primitive, offset),
            _ => return None,
        };
        let offset = match offset {
            OffsetExpr::Base => 0,
            OffsetExpr::BasePlus(offset) => *offset,
            _ => return None,
        };
        let name = NamingConvention::field_name(field.name.as_str());
        let const_name = NamingConvention::enum_constant_name(field.name.as_str());
        Some(JavaBlittableField {
            name: name.clone(),
            const_name: const_name.clone(),
            offset,
            decode_expr: java_blittable_decode_expr(primitive, &const_name),
            encode_expr: java_blittable_encode_expr(primitive, &const_name, &name),
        })
    }

    fn lower_record_field(&self, record_id: &RecordId, field: &FieldDef) -> JavaRecordField {
        let decode_seq = self
            .record_field_read_seq(record_id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(record_id, &field.name)
            .expect("record field encode ops");
        JavaRecordField {
            name: NamingConvention::field_name(field.name.as_str()),
            java_type: self.java_type(&field.type_expr),
            wire_decode_expr: super::emit::emit_reader_read(&decode_seq),
            wire_size_expr: super::emit::emit_size_expr(&encode_seq.size),
            wire_encode_expr: super::emit::emit_write_expr(&encode_seq, "wire"),
            equals_expr: self.record_field_equals_expr(&field.type_expr, field.name.as_str()),
            hash_expr: self.record_field_hash_expr(&field.type_expr, field.name.as_str()),
        }
    }

    fn record_field_equals_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        let left = format!("this.{field}");
        let right = format!("other.{field}");
        self.value_equals_expr(ty, &left, &right)
    }

    fn record_field_hash_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        self.value_hash_expr(ty, &field)
    }

    fn value_equals_expr(&self, ty: &TypeExpr, left: &str, right: &str) -> String {
        self.value_equals_expr_with_depth(ty, left, right, 0)
    }

    fn value_equals_expr_with_depth(
        &self,
        ty: &TypeExpr,
        left: &str,
        right: &str,
        depth: usize,
    ) -> String {
        match ty {
            TypeExpr::Primitive(PrimitiveType::F32) => {
                format!("Float.compare({left}, {right}) == 0")
            }
            TypeExpr::Primitive(PrimitiveType::F64) => {
                format!("Double.compare({left}, {right}) == 0")
            }
            TypeExpr::Primitive(_) => format!("{left} == {right}"),
            TypeExpr::String | TypeExpr::Record(_) | TypeExpr::Enum(_) => {
                format!("java.util.Objects.equals({left}, {right})")
            }
            TypeExpr::Option(inner) => {
                let left_is_null = format!("({left}) == null");
                let right_is_null = format!("({right}) == null");
                let left_present = format!("({left}).isPresent()");
                let right_present = format!("({right}).isPresent()");
                let left_value = format!("({left}).get()");
                let right_value = format!("({right}).get()");
                let inner_equals =
                    self.value_equals_expr_with_depth(inner, &left_value, &right_value, depth);
                format!(
                    "({left_is_null} ? {right_is_null} : (!({right_is_null}) && {left_present} == {right_present} && (!({left_present}) || {inner_equals})))"
                )
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => format!("java.util.Arrays.equals({left}, {right})"),
                _ => {
                    let left_item = format!("leftItem{depth}");
                    let right_item = format!("rightItem{depth}");
                    let inner_equals = self.value_equals_expr_with_depth(
                        inner,
                        &left_item,
                        &right_item,
                        depth + 1,
                    );
                    format!(
                        "WireWriter.listEquals({left}, {right}, ({left_item}, {right_item}) -> {inner_equals})"
                    )
                }
            },
            _ => panic!("unsupported Java equality type: {:?}", ty),
        }
    }

    fn value_hash_expr(&self, ty: &TypeExpr, value: &str) -> String {
        self.value_hash_expr_with_depth(ty, value, 0)
    }

    fn value_hash_expr_with_depth(&self, ty: &TypeExpr, value: &str, depth: usize) -> String {
        match ty {
            TypeExpr::Primitive(PrimitiveType::Bool) => format!("Boolean.hashCode({value})"),
            TypeExpr::Primitive(PrimitiveType::I8) | TypeExpr::Primitive(PrimitiveType::U8) => {
                format!("Byte.hashCode({value})")
            }
            TypeExpr::Primitive(PrimitiveType::I16) | TypeExpr::Primitive(PrimitiveType::U16) => {
                format!("Short.hashCode({value})")
            }
            TypeExpr::Primitive(PrimitiveType::I32) | TypeExpr::Primitive(PrimitiveType::U32) => {
                format!("Integer.hashCode({value})")
            }
            TypeExpr::Primitive(PrimitiveType::I64)
            | TypeExpr::Primitive(PrimitiveType::U64)
            | TypeExpr::Primitive(PrimitiveType::ISize)
            | TypeExpr::Primitive(PrimitiveType::USize) => format!("Long.hashCode({value})"),
            TypeExpr::Primitive(PrimitiveType::F32) => format!("Float.hashCode({value})"),
            TypeExpr::Primitive(PrimitiveType::F64) => format!("Double.hashCode({value})"),
            TypeExpr::String | TypeExpr::Record(_) | TypeExpr::Enum(_) => {
                format!("java.util.Objects.hashCode({value})")
            }
            TypeExpr::Option(inner) => {
                let inner_value = format!("({value}).get()");
                let inner_hash = self.value_hash_expr_with_depth(inner, &inner_value, depth);
                format!(
                    "(({value}) == null ? 0 : (({value}).isPresent() ? (31 + ({inner_hash})) : 0))"
                )
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => format!("java.util.Arrays.hashCode({value})"),
                _ => {
                    let item = format!("item{depth}");
                    let inner_hash = self.value_hash_expr_with_depth(inner, &item, depth + 1);
                    format!("WireWriter.listHash({value}, {item} -> {inner_hash})")
                }
            },
            _ => panic!("unsupported Java hash type: {:?}", ty),
        }
    }

    fn record_field_read_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<ReadSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.decode_ops.ops.first() {
                Some(ReadOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn record_field_write_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<WriteSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.encode_ops.ops.first() {
                Some(WriteOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn lower_function(&self, func: &FunctionDef) -> JavaFunction {
        let call = self.abi_call_for_function(func);

        let wire_writers = self.wire_writers_for_params(call);

        let params: Vec<JavaParam> = func
            .params
            .iter()
            .enumerate()
            .map(|(parameter_index, parameter)| {
                self.lower_param(
                    parameter.name.as_str(),
                    &parameter.type_expr,
                    parameter_index,
                    call,
                    &wire_writers,
                )
            })
            .collect();

        let strategy = self.return_strategy(&func.returns, call);

        JavaFunction {
            name: NamingConvention::method_name(func.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            params,
            return_type: self.return_java_type(&func.returns),
            strategy,
            wire_writers,
        }
    }

    fn lower_param(
        &self,
        name: &str,
        ty: &TypeExpr,
        parameter_index: usize,
        call: &AbiCall,
        wire_writers: &[JavaWireWriter],
    ) -> JavaParam {
        let field_name = NamingConvention::field_name(name);
        let java_type = self.java_type(ty);
        let abi_transport = call
            .params
            .iter()
            .find(|p| p.name.as_str() == name)
            .and_then(|p| p.transport());

        let (native_type, native_expr) = match ty {
            TypeExpr::String => (
                "byte[]".to_string(),
                format!(
                    "{}.getBytes(java.nio.charset.StandardCharsets.UTF_8)",
                    field_name
                ),
            ),
            TypeExpr::Enum(_)
                if matches!(
                    abi_transport,
                    Some(Transport::Scalar(ScalarOrigin::CStyleEnum { .. }))
                ) =>
            {
                let tag_type = match abi_transport {
                    Some(Transport::Scalar(ScalarOrigin::CStyleEnum { tag_type, .. })) => *tag_type,
                    _ => unreachable!(),
                };
                (
                    mappings::java_type(tag_type).to_string(),
                    format!("{}.value", field_name),
                )
            }
            TypeExpr::Vec(inner) => {
                if let Some(Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                    primitive,
                )))) = abi_transport
                    && matches!(
                        (inner.as_ref(), primitive),
                        (
                            TypeExpr::Primitive(PrimitiveType::ISize | PrimitiveType::USize),
                            PrimitiveType::ISize | PrimitiveType::USize
                        )
                    )
                {
                    let native_expr =
                        vec_pointer_sized_primitive_param_encode_expr(&field_name, parameter_index);
                    return JavaParam {
                        name: field_name,
                        java_type,
                        native_type: "ByteBuffer".to_string(),
                        native_expr,
                    };
                }
                if let Some(Transport::Span(SpanContent::Scalar(ScalarOrigin::CStyleEnum {
                    tag_type,
                    ..
                }))) = abi_transport
                {
                    let native_expr = vec_c_style_enum_param_encode_expr(&field_name, *tag_type);
                    return JavaParam {
                        name: field_name,
                        java_type,
                        native_type: mappings::java_primitive_array_type(*tag_type).to_string(),
                        native_expr,
                    };
                }
                let has_wire_writer = wire_writers.iter().any(|w| w.param_name == name);
                if has_wire_writer {
                    let binding_name = wire_writers
                        .iter()
                        .find(|w| w.param_name == name)
                        .map(|w| w.binding_name.as_str())
                        .unwrap_or("");
                    (
                        "ByteBuffer".to_string(),
                        format!("{}.toBuffer()", binding_name),
                    )
                } else {
                    (java_type.clone(), field_name.clone())
                }
            }
            TypeExpr::Record(_) | TypeExpr::Enum(_) | TypeExpr::Option(_) => {
                let binding_name = wire_writers
                    .iter()
                    .find(|w| w.param_name == name)
                    .map(|w| w.binding_name.as_str())
                    .unwrap_or("");
                (
                    "ByteBuffer".to_string(),
                    format!("{}.toBuffer()", binding_name),
                )
            }
            _ => (java_type.clone(), field_name.clone()),
        };

        JavaParam {
            name: field_name,
            java_type,
            native_type,
            native_expr,
        }
    }

    fn wire_writers_for_params(&self, call: &AbiCall) -> Vec<JavaWireWriter> {
        call.params
            .iter()
            .filter_map(|param| {
                self.input_write_ops(param).map(|encode_ops| {
                    let param_name = param.name.as_str().to_string();
                    let binding_name = format!("_wire_{}", param.name.as_str());
                    let encode_expr = super::emit::emit_write_expr(&encode_ops, &binding_name);
                    JavaWireWriter {
                        binding_name,
                        param_name,
                        size_expr: super::emit::emit_size_expr(&encode_ops.size),
                        encode_expr,
                    }
                })
            })
            .collect()
    }

    fn input_write_ops(&self, param: &AbiParam) -> Option<WriteSeq> {
        match &param.role {
            ParamRole::Input {
                encode_ops: Some(encode_ops),
                ..
            } => Some(encode_ops.clone()),
            _ => None,
        }
    }

    fn return_java_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "void".to_string(),
            ReturnDef::Value(TypeExpr::Void) => "void".to_string(),
            ReturnDef::Value(ty) => self.java_type(ty),
            ReturnDef::Result { .. } => "void".to_string(),
        }
    }

    fn return_strategy(&self, returns: &ReturnDef, call: &AbiCall) -> JavaReturnStrategy {
        match returns {
            ReturnDef::Void | ReturnDef::Result { .. } => JavaReturnStrategy::Void,
            ReturnDef::Value(ty) => match ty {
                TypeExpr::Void => JavaReturnStrategy::Void,
                TypeExpr::Primitive(_) => JavaReturnStrategy::Direct,
                TypeExpr::String => JavaReturnStrategy::WireDecode {
                    decode_expr: "reader.readString()".to_string(),
                },
                TypeExpr::Option(_) => match &call.returns.decode_ops {
                    Some(decode_seq) => JavaReturnStrategy::WireDecode {
                        decode_expr: super::emit::emit_reader_read(decode_seq),
                    },
                    None => panic!(
                        "unsupported direct Option return transport for Java backend: {:?}",
                        ty
                    ),
                },
                TypeExpr::Record(id) => JavaReturnStrategy::WireDecode {
                    decode_expr: format!(
                        "{}.decode(reader)",
                        NamingConvention::class_name(id.as_str())
                    ),
                },
                TypeExpr::Enum(id) => {
                    if let Some(Transport::Scalar(ScalarOrigin::CStyleEnum { tag_type, .. })) =
                        call.returns.transport.as_ref()
                    {
                        JavaReturnStrategy::CStyleEnumDecode {
                            class_name: NamingConvention::class_name(id.as_str()),
                            native_type: mappings::java_type(*tag_type).to_string(),
                        }
                    } else {
                        JavaReturnStrategy::WireDecode {
                            decode_expr: format!(
                                "{}.decode(reader)",
                                NamingConvention::class_name(id.as_str())
                            ),
                        }
                    }
                }
                TypeExpr::Vec(inner) => match &call.returns.decode_ops {
                    Some(decode_seq) => JavaReturnStrategy::WireDecode {
                        decode_expr: super::emit::emit_reader_read(decode_seq),
                    },
                    None => JavaReturnStrategy::BufferDecode {
                        decode_expr: self
                            .vec_buffer_decode_expr(inner, call.returns.transport.as_ref()),
                    },
                },
                _ => JavaReturnStrategy::Void,
            },
        }
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| matches!(&c.id, CallId::Function(id) if id == &func.id))
            .expect("abi call not found for function")
    }

    fn vec_buffer_decode_expr(&self, inner: &TypeExpr, transport: Option<&Transport>) -> String {
        match transport {
            Some(Transport::Span(SpanContent::Scalar(origin))) => match origin {
                ScalarOrigin::Primitive(primitive) => vec_primitive_buffer_decode(*primitive),
                ScalarOrigin::CStyleEnum { enum_id, tag_type } => {
                    vec_c_style_enum_buffer_decode(enum_id.as_str(), *tag_type)
                }
            },
            Some(Transport::Span(SpanContent::Composite(layout))) => {
                format!(
                    "{}.decodeBlittableVecFromRawBuffer(_buf)",
                    NamingConvention::class_name(layout.record_id.as_str()),
                )
            }
            _ => match inner {
                TypeExpr::Primitive(primitive) => vec_primitive_buffer_decode(*primitive),
                TypeExpr::Record(id) => match self.ffi.catalog.resolve_record(id) {
                    Some(record) if record.is_blittable() => format!(
                        "{}.decodeBlittableVecFromRawBuffer(_buf)",
                        NamingConvention::class_name(id.as_str()),
                    ),
                    _ => panic!(
                        "unsupported direct Vec<Record> return transport for non-blittable record: {:?}",
                        id
                    ),
                },
                _ => panic!(
                    "unsupported direct Vec return transport for Java backend: {:?}",
                    inner
                ),
            },
        }
    }

    fn java_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Option(inner) => {
                format!("java.util.Optional<{}>", self.java_boxed_type(inner))
            }
            TypeExpr::Vec(inner) => self.java_vec_type(inner),
            _ => "Object".to_string(),
        }
    }

    fn java_vec_type(&self, inner: &TypeExpr) -> String {
        match inner {
            TypeExpr::Primitive(p) => mappings::java_primitive_array_type(*p).to_string(),
            _ => format!("java.util.List<{}>", self.java_boxed_type(inner)),
        }
    }

    fn java_boxed_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_boxed_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Option(inner) => {
                format!("java.util.Optional<{}>", self.java_boxed_type(inner))
            }
            TypeExpr::Vec(inner) => self.java_vec_type(inner),
            _ => "Object".to_string(),
        }
    }

    fn lower_enum(&self, enumeration: &EnumDef) -> JavaEnum {
        let abi_enum = self.abi_enum_for(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let kind = if abi_enum.is_c_style {
            JavaEnumKind::CStyle
        } else if self.options.min_java_version.supports_sealed()
            && !Self::requires_manual_enum_value_semantics(abi_enum)
        {
            JavaEnumKind::SealedInterface
        } else {
            JavaEnumKind::AbstractClass
        };
        let value_type = match &enumeration.repr {
            EnumRepr::CStyle { tag_type, .. } | EnumRepr::Data { tag_type, .. } => {
                mappings::java_type(*tag_type).to_string()
            }
        };
        let variant_names: HashSet<String> = abi_enum
            .variants
            .iter()
            .map(|v| NamingConvention::class_name(v.name.as_str()))
            .collect();
        let variants = abi_enum
            .variants
            .iter()
            .map(|variant| self.lower_enum_variant(variant, kind, &variant_names))
            .collect();
        JavaEnum {
            class_name,
            kind,
            value_type,
            variants,
        }
    }

    fn requires_manual_enum_value_semantics(enumeration: &AbiEnum) -> bool {
        enumeration
            .variants
            .iter()
            .any(|variant| match &variant.payload {
                AbiEnumPayload::Unit => false,
                AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                    .iter()
                    .any(|field| Self::contains_primitive_array_component(&field.type_expr)),
            })
    }

    fn lower_enum_variant(
        &self,
        variant: &AbiEnumVariant,
        kind: JavaEnumKind,
        sibling_names: &HashSet<String>,
    ) -> JavaEnumVariant {
        let name = match kind {
            JavaEnumKind::CStyle => NamingConvention::enum_constant_name(variant.name.as_str()),
            _ => NamingConvention::class_name(variant.name.as_str()),
        };
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|field| self.lower_enum_field(field, sibling_names))
                .collect(),
        };
        JavaEnumVariant {
            name,
            tag: variant.discriminant,
            fields,
        }
    }

    fn lower_enum_field(
        &self,
        field: &AbiEnumField,
        sibling_names: &HashSet<String>,
    ) -> JavaEnumField {
        let field_name = NamingConvention::field_name(field.name.as_str());
        let equals_expr = self.value_equals_expr(
            &field.type_expr,
            &format!("this.{field_name}"),
            &format!("other.{field_name}"),
        );
        let hash_expr = self.value_hash_expr(&field.type_expr, field_name.as_str());
        let prefixed = Self::prefix_write_seq(&field.encode, "_v");
        let mut java_type = self.java_type(&field.type_expr);
        let mut decode_expr = super::emit::emit_reader_read(&field.decode);
        let mut size_expr = super::emit::emit_size_expr(&prefixed.size);
        let mut encode_expr = super::emit::emit_write_expr(&prefixed, "wire");
        if sibling_names.contains(&java_type) {
            java_type = format!("{}.{}", self.package_name, java_type);
        }
        self.qualify_colliding_names(&mut decode_expr, sibling_names);
        self.qualify_colliding_names(&mut size_expr, sibling_names);
        self.qualify_colliding_names(&mut encode_expr, sibling_names);
        JavaEnumField {
            name: field_name,
            java_type,
            wire_decode_expr: decode_expr,
            wire_size_expr: size_expr,
            wire_encode_expr: encode_expr,
            equals_expr,
            hash_expr,
        }
    }

    fn qualify_colliding_names(&self, expr: &mut String, sibling_names: &HashSet<String>) {
        for name in sibling_names {
            let pattern = format!("{}.decode(", name);
            if expr.contains(&pattern) {
                let qualified = format!("{}.{}.decode(", self.package_name, name);
                *expr = expr.replace(&pattern, &qualified);
            }
            let pattern = format!("{}.wireEncodeTo(", name);
            if expr.contains(&pattern) {
                let qualified = format!("{}.{}.wireEncodeTo(", self.package_name, name);
                *expr = expr.replace(&pattern, &qualified);
            }
            let pattern = format!("{}.wireEncodedSize(", name);
            if expr.contains(&pattern) {
                let qualified = format!("{}.{}.wireEncodedSize(", self.package_name, name);
                *expr = expr.replace(&pattern, &qualified);
            }
        }
    }

    fn prefix_value(value: &ValueExpr, binding: &str) -> ValueExpr {
        match value {
            ValueExpr::Instance => ValueExpr::Var(binding.to_string()),
            ValueExpr::Named(name) => ValueExpr::Field(
                Box::new(ValueExpr::Var(binding.to_string())),
                FieldName::new(name),
            ),
            ValueExpr::Var(_) => value.clone(),
            ValueExpr::Field(parent, field) => {
                ValueExpr::Field(Box::new(Self::prefix_value(parent, binding)), field.clone())
            }
        }
    }

    fn prefix_write_op(op: &WriteOp, binding: &str) -> WriteOp {
        match op {
            WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
                primitive: *primitive,
                value: Self::prefix_value(value, binding),
            },
            WriteOp::String { value } => WriteOp::String {
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Bytes { value } => WriteOp::Bytes {
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Option { value, some } => WriteOp::Option {
                value: Self::prefix_value(value, binding),
                some: Box::new(Self::prefix_write_seq(some, binding)),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                fields: fields
                    .iter()
                    .map(|f| FieldWriteOp {
                        name: f.name.clone(),
                        accessor: Self::prefix_value(&f.accessor, binding),
                        seq: Self::prefix_write_seq(&f.seq, binding),
                    })
                    .collect(),
            },
            WriteOp::Enum { id, value, layout } => WriteOp::Enum {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                layout: layout.clone(),
            },
            WriteOp::Result { value, ok, err } => WriteOp::Result {
                value: Self::prefix_value(value, binding),
                ok: Box::new(Self::prefix_write_seq(ok, binding)),
                err: Box::new(Self::prefix_write_seq(err, binding)),
            },
            WriteOp::Builtin { id, value } => WriteOp::Builtin {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Custom {
                id,
                value,
                underlying,
            } => WriteOp::Custom {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                underlying: Box::new(Self::prefix_write_seq(underlying, binding)),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: Self::prefix_value(value, binding),
                element_type: element_type.clone(),
                element: element.clone(),
                layout: layout.clone(),
            },
        }
    }

    fn prefix_size_expr(size: &SizeExpr, binding: &str) -> SizeExpr {
        match size {
            SizeExpr::Fixed(_) | SizeExpr::Runtime => size.clone(),
            SizeExpr::StringLen(v) => SizeExpr::StringLen(Self::prefix_value(v, binding)),
            SizeExpr::BytesLen(v) => SizeExpr::BytesLen(Self::prefix_value(v, binding)),
            SizeExpr::ValueSize(v) => SizeExpr::ValueSize(Self::prefix_value(v, binding)),
            SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
            },
            SizeExpr::WireSize { value, record_id } => SizeExpr::WireSize {
                value: Self::prefix_value(value, binding),
                record_id: record_id.clone(),
            },
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: Self::prefix_value(value, binding),
                inner: Box::new(Self::prefix_size_expr(inner, binding)),
            },
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: Self::prefix_value(value, binding),
                inner: Box::new(Self::prefix_size_expr(inner, binding)),
                layout: layout.clone(),
            },
            SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
                value: Self::prefix_value(value, binding),
                ok: Box::new(Self::prefix_size_expr(ok, binding)),
                err: Box::new(Self::prefix_size_expr(err, binding)),
            },
            SizeExpr::Sum(parts) => SizeExpr::Sum(
                parts
                    .iter()
                    .map(|p| Self::prefix_size_expr(p, binding))
                    .collect(),
            ),
        }
    }

    fn prefix_write_seq(seq: &WriteSeq, binding: &str) -> WriteSeq {
        WriteSeq {
            size: Self::prefix_size_expr(&seq.size, binding),
            ops: seq
                .ops
                .iter()
                .map(|op| Self::prefix_write_op(op, binding))
                .collect(),
            shape: seq.shape,
        }
    }

    fn abi_enum_for(&self, enumeration: &EnumDef) -> &AbiEnum {
        self.abi
            .enums
            .iter()
            .find(|abi_enum| abi_enum.id == enumeration.id)
            .expect("abi enum missing")
    }
}

fn vec_primitive_buffer_decode(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "WireReader.booleanArrayFromRawBuffer(_buf)".to_string(),
        PrimitiveType::I8 | PrimitiveType::U8 => "_buf != null ? _buf : new byte[0]".to_string(),
        PrimitiveType::I16 | PrimitiveType::U16 => {
            "WireReader.shortArrayFromRawBuffer(_buf)".to_string()
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            "WireReader.intArrayFromRawBuffer(_buf)".to_string()
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "WireReader.longArrayFromRawBuffer(_buf)".to_string()
        }
        PrimitiveType::F32 => "WireReader.floatArrayFromRawBuffer(_buf)".to_string(),
        PrimitiveType::F64 => "WireReader.doubleArrayFromRawBuffer(_buf)".to_string(),
    }
}

fn java_blittable_decode_expr(primitive: PrimitiveType, const_name: &str) -> String {
    let offset = format!("OFFSET_{}", const_name);
    match primitive {
        PrimitiveType::Bool => format!("buf.get(base + {}) != 0", offset),
        PrimitiveType::I8 | PrimitiveType::U8 => format!("buf.get(base + {})", offset),
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!("buf.getShort(base + {})", offset)
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!("buf.getInt(base + {})", offset)
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("buf.getLong(base + {})", offset)
        }
        PrimitiveType::F32 => format!("buf.getFloat(base + {})", offset),
        PrimitiveType::F64 => format!("buf.getDouble(base + {})", offset),
    }
}

fn java_blittable_encode_expr(
    primitive: PrimitiveType,
    const_name: &str,
    field_name: &str,
) -> String {
    let offset = format!("OFFSET_{}", const_name);
    let accessor = format!("item.{}()", field_name);
    match primitive {
        PrimitiveType::Bool => {
            format!("buf.put(base + {}, (byte) ({} ? 1 : 0))", offset, accessor)
        }
        PrimitiveType::I8 | PrimitiveType::U8 => {
            format!("buf.put(base + {}, {})", offset, accessor)
        }
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!("buf.putShort(base + {}, {})", offset, accessor)
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!("buf.putInt(base + {}, {})", offset, accessor)
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("buf.putLong(base + {}, {})", offset, accessor)
        }
        PrimitiveType::F32 => {
            format!("buf.putFloat(base + {}, {})", offset, accessor)
        }
        PrimitiveType::F64 => {
            format!("buf.putDouble(base + {}, {})", offset, accessor)
        }
    }
}

fn vec_c_style_enum_buffer_decode(enum_id: &str, tag_type: PrimitiveType) -> String {
    let class_name = NamingConvention::class_name(enum_id);
    match tag_type {
        PrimitiveType::I8 | PrimitiveType::U8 => {
            format!(
                "WireReader.mapByteArray(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!(
                "WireReader.mapShortRawBuffer(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!(
                "WireReader.mapIntRawBuffer(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!(
                "WireReader.mapLongRawBuffer(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        _ => panic!(
            "unsupported C-style enum tag type for Vec decode: {:?}",
            tag_type
        ),
    }
}

fn vec_c_style_enum_param_encode_expr(name: &str, tag_type: PrimitiveType) -> String {
    match tag_type {
        PrimitiveType::I8 | PrimitiveType::U8 => {
            format!("WireReader.toByteArray({}, item -> item.value)", name)
        }
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!("WireReader.toShortArray({}, item -> item.value)", name)
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!("WireReader.toIntArray({}, item -> item.value)", name)
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("WireReader.toLongArray({}, item -> item.value)", name)
        }
        _ => panic!(
            "unsupported C-style enum tag type for Vec param encode: {:?}",
            tag_type
        ),
    }
}

fn vec_pointer_sized_primitive_param_encode_expr(name: &str, slot: usize) -> String {
    format!("WireReader.encodeLongVecInput({}, {})", name, slot)
}
