use std::collections::HashSet;

use super::JavaOptions;
use super::mappings;
use super::names::NamingConvention;
use super::plan::{
    JavaEnum, JavaEnumField, JavaEnumKind, JavaEnumVariant, JavaFunction, JavaModule, JavaParam,
    JavaRecord, JavaRecordField, JavaRecordShape, JavaReturnStrategy, JavaWireWriter,
};
use crate::ir::abi::{
    AbiCall, AbiContract, AbiEnum, AbiEnumField, AbiEnumPayload, AbiEnumVariant, AbiParam,
    AbiRecord, CallId, ParamRole,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{EnumDef, EnumRepr, FieldDef, FunctionDef, RecordDef, ReturnDef};
use crate::ir::ids::{FieldName, RecordId};
use crate::ir::ops::{FieldWriteOp, ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::plan::{ScalarOrigin, Transport};
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
        let shape = if self.options.min_java_version.supports_records() {
            JavaRecordShape::NativeRecord
        } else {
            JavaRecordShape::ClassicClass
        };
        JavaRecord {
            shape,
            class_name,
            fields,
        }
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
            wire_size_expr: super::emit::emit_size_expr_for_write_seq(&encode_seq),
            wire_encode_expr: super::emit::emit_write_expr(&encode_seq, "wire"),
            equals_expr: self.record_field_equals_expr(&field.type_expr, field.name.as_str()),
            hash_expr: self.record_field_hash_expr(&field.type_expr, field.name.as_str()),
        }
    }

    fn record_field_equals_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        match ty {
            TypeExpr::Primitive(PrimitiveType::F32) => {
                format!("Float.compare(this.{field}, other.{field}) == 0")
            }
            TypeExpr::Primitive(PrimitiveType::F64) => {
                format!("Double.compare(this.{field}, other.{field}) == 0")
            }
            TypeExpr::Primitive(_) => format!("this.{field} == other.{field}"),
            TypeExpr::String | TypeExpr::Record(_) | TypeExpr::Enum(_) => {
                format!("Objects.equals(this.{field}, other.{field})")
            }
            _ => panic!("unsupported Java record field equality type: {:?}", ty),
        }
    }

    fn record_field_hash_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        match ty {
            TypeExpr::Primitive(PrimitiveType::Bool) => format!("Boolean.hashCode({field})"),
            TypeExpr::Primitive(PrimitiveType::I8) | TypeExpr::Primitive(PrimitiveType::U8) => {
                format!("Byte.hashCode({field})")
            }
            TypeExpr::Primitive(PrimitiveType::I16) | TypeExpr::Primitive(PrimitiveType::U16) => {
                format!("Short.hashCode({field})")
            }
            TypeExpr::Primitive(PrimitiveType::I32) | TypeExpr::Primitive(PrimitiveType::U32) => {
                format!("Integer.hashCode({field})")
            }
            TypeExpr::Primitive(PrimitiveType::I64)
            | TypeExpr::Primitive(PrimitiveType::U64)
            | TypeExpr::Primitive(PrimitiveType::ISize)
            | TypeExpr::Primitive(PrimitiveType::USize) => format!("Long.hashCode({field})"),
            TypeExpr::Primitive(PrimitiveType::F32) => format!("Float.hashCode({field})"),
            TypeExpr::Primitive(PrimitiveType::F64) => format!("Double.hashCode({field})"),
            TypeExpr::String | TypeExpr::Record(_) | TypeExpr::Enum(_) => {
                format!("Objects.hashCode({field})")
            }
            _ => panic!("unsupported Java record field hash type: {:?}", ty),
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
            .map(|p| self.lower_param(p.name.as_str(), &p.type_expr, call, &wire_writers))
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
            TypeExpr::Record(_) | TypeExpr::Enum(_) => {
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
                        size_expr: super::emit::emit_size_expr_for_write_seq(&encode_ops),
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

    fn java_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            _ => "Object".to_string(),
        }
    }

    fn lower_enum(&self, enumeration: &EnumDef) -> JavaEnum {
        let abi_enum = self.abi_enum_for(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let kind = if abi_enum.is_c_style {
            JavaEnumKind::CStyle
        } else if self.options.min_java_version.supports_sealed() {
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
        let prefixed = Self::prefix_write_seq(&field.encode, "_v");
        let mut java_type = self.java_type(&field.type_expr);
        let mut decode_expr = super::emit::emit_reader_read(&field.decode);
        let mut size_expr = super::emit::emit_size_expr_for_write_seq(&prefixed);
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
            other => other.clone(),
        }
    }

    fn prefix_size_expr(size: &SizeExpr, binding: &str) -> SizeExpr {
        match size {
            SizeExpr::Fixed(_) | SizeExpr::Runtime => size.clone(),
            SizeExpr::StringLen(v) => SizeExpr::StringLen(Self::prefix_value(v, binding)),
            SizeExpr::BytesLen(v) => SizeExpr::BytesLen(Self::prefix_value(v, binding)),
            SizeExpr::ValueSize(v) => SizeExpr::ValueSize(Self::prefix_value(v, binding)),
            SizeExpr::WireSize { value, record_id } => SizeExpr::WireSize {
                value: Self::prefix_value(value, binding),
                record_id: record_id.clone(),
            },
            SizeExpr::Sum(parts) => SizeExpr::Sum(
                parts
                    .iter()
                    .map(|p| Self::prefix_size_expr(p, binding))
                    .collect(),
            ),
            other => other.clone(),
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
