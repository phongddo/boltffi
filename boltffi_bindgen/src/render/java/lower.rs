use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::transport::{
    EncodedReturnStrategy, ReturnInvocationContext, ReturnPlatform, ScalarReturnStrategy,
    ValueReturnMethod, ValueReturnStrategy,
};

use super::JavaOptions;
use super::mappings;
use super::names::NamingConvention;
use super::plan::{
    JavaAsyncCall, JavaAsyncMode, JavaBlittableField, JavaBlittableLayout, JavaCallbackMethod,
    JavaCallbackParam, JavaCallbackReturn, JavaCallbackTrait, JavaClass, JavaClassMethod,
    JavaClosureInterface, JavaClosureParam, JavaConstructor, JavaConstructorKind, JavaEnum,
    JavaEnumField, JavaEnumKind, JavaEnumVariant, JavaFunction, JavaModule, JavaParam, JavaRecord,
    JavaRecordField, JavaRecordShape, JavaReturnPlan, JavaReturnRender, JavaWireWriter,
};
use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, CallId, CallMode, ParamRole,
};
use crate::ir::codec::EnumTagStrategy;
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, EnumDef, EnumRepr,
    FieldDef, FunctionDef, MethodDef, Receiver, RecordDef, ReturnDef,
};
use crate::ir::ids::{CallbackId, FieldName, RecordId};
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
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Bytes | TypeExpr::Void => true,
            TypeExpr::Record(id) => supported.contains(id.as_str()),
            TypeExpr::Enum(id) => supported.contains(id.as_str()),
            TypeExpr::Option(inner) => Self::is_leaf_supported(inner, supported),
            TypeExpr::Vec(inner) => Self::is_leaf_supported(inner, supported),
            TypeExpr::Callback(_) => true,
            TypeExpr::Handle(_) => true,
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
            .filter(|f| self.is_supported_function(f))
            .map(|f| self.lower_function(f))
            .collect();

        let closures: Vec<JavaClosureInterface> = self
            .ffi
            .catalog
            .all_callbacks()
            .filter(|cb| matches!(cb.kind, CallbackKind::Closure))
            .map(|cb| self.lower_closure(cb))
            .collect();

        let callbacks: Vec<JavaCallbackTrait> = self
            .ffi
            .catalog
            .all_callbacks()
            .filter(|cb| matches!(cb.kind, CallbackKind::Trait))
            .filter(|cb| !cb.methods.is_empty())
            .map(|cb| self.lower_callback_trait(cb))
            .collect();

        let classes: Vec<JavaClass> = self
            .ffi
            .catalog
            .all_classes()
            .map(|c| self.lower_class(c))
            .collect();

        JavaModule {
            package_name: self.package_name.clone(),
            class_name: NamingConvention::class_name(&self.module_name),
            lib_name,
            java_version: self.options.min_java_version,
            async_mode: JavaAsyncMode::from_version(self.options.min_java_version),
            prefix,
            records,
            enums,
            closures,
            callbacks,
            functions,
            classes,
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
            ReturnDef::Result { ok, err } => {
                self.is_supported_result_type(ok) && self.is_supported_result_type(err)
            }
        };
        params_ok && return_ok
    }

    fn is_supported_result_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Void => true,
            TypeExpr::Handle(_) => false,
            _ => self.is_supported_type(ty),
        }
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
            TypeExpr::Bytes => true,
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
            TypeExpr::Bytes => format!("java.util.Arrays.equals({left}, {right})"),
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
            TypeExpr::Bytes => format!("java.util.Arrays.hashCode({value})"),
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

        let async_call = self.async_call_from_mode(call, &func.returns);
        let return_plan = match &async_call {
            Some(async_call) => async_call.complete_return_plan.clone(),
            None => self.return_plan(&func.returns, call),
        };

        JavaFunction {
            name: NamingConvention::method_name(func.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            params,
            return_type: self.return_java_type(&func.returns),
            return_plan,
            wire_writers,
            async_call,
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
            TypeExpr::Bytes => ("byte[]".to_string(), field_name.clone()),
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
            TypeExpr::Callback(callback_id) => {
                let bridge_name = self.callback_bridge_name(callback_id);
                (
                    "long".to_string(),
                    format!("{}.create({})", bridge_name, field_name),
                )
            }
            TypeExpr::Handle(_) => ("long".to_string(), format!("{}.handle", field_name)),
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
            ReturnDef::Result {
                ok: TypeExpr::Void, ..
            } => "void".to_string(),
            ReturnDef::Result { ok, .. } => self.java_type(ok),
        }
    }

    fn result_return_plan(&self, returns: &ReturnDef, call: &AbiCall) -> JavaReturnPlan {
        let decode_ops = call
            .returns
            .decode_ops
            .as_ref()
            .expect("Result return should have decode ops");
        let (ok_seq, err_seq) = match decode_ops.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => (ok.as_ref(), err.as_ref()),
            _ => panic!("expected ReadOp::Result in decode ops"),
        };
        let ok_decode_expr = super::emit::emit_reader_read(ok_seq);
        let err_decode_expr = super::emit::emit_reader_read(err_seq);
        let err_is_string =
            matches!(returns, ReturnDef::Result { err, .. } if matches!(err, TypeExpr::String));
        let err_exception_class = match returns {
            ReturnDef::Result {
                err: TypeExpr::Enum(id),
                ..
            } => self
                .ffi
                .catalog
                .resolve_enum(id)
                .filter(|e| e.is_error)
                .map(|_| format!("{}.Exception", NamingConvention::class_name(id.as_str()))),
            _ => None,
        };
        JavaReturnPlan {
            native_return_type: "byte[]".to_string(),
            render: JavaReturnRender::Result {
                ok_decode_expr,
                err_decode_expr,
                err_is_string,
                err_exception_class,
            },
        }
    }

    fn return_plan(&self, returns: &ReturnDef, call: &AbiCall) -> JavaReturnPlan {
        match returns {
            ReturnDef::Void => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
            ReturnDef::Result { .. } => self.result_return_plan(returns, call),
            ReturnDef::Value(ty) => self.java_return_plan_for_value(ty, call),
        }
    }

    fn java_return_plan_for_value(&self, ty: &TypeExpr, call: &AbiCall) -> JavaReturnPlan {
        let value_return_strategy = call.returns.value_return_strategy();
        let value_return_method = call.returns.value_return_method(
            &call.error,
            ReturnInvocationContext::HostCall,
            ReturnPlatform::Native,
        );
        match ty {
            TypeExpr::Void => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
            _ => match (value_return_strategy, value_return_method) {
                (ValueReturnStrategy::Void, _) => JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                },
                (
                    ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue),
                    ValueReturnMethod::DirectReturn,
                ) => JavaReturnPlan {
                    native_return_type: self.java_type(ty),
                    render: JavaReturnRender::Direct,
                },
                (
                    ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag),
                    ValueReturnMethod::DirectReturn,
                ) => self.java_c_style_enum_return_plan(ty, call),
                (
                    ValueReturnStrategy::CompositeValue
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String)
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::ResultScalar)
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded),
                    ValueReturnMethod::DirectReturn | ValueReturnMethod::WriteToOutBufferParts,
                ) => self.java_decode_return_plan(ty, call),
                (
                    ValueReturnStrategy::Buffer(EncodedReturnStrategy::DirectVec),
                    ValueReturnMethod::DirectReturn | ValueReturnMethod::WriteToOutBufferParts,
                ) => self.java_direct_vec_return_plan(ty, call),
                (ValueReturnStrategy::ObjectHandle, ValueReturnMethod::DirectReturn) => {
                    self.java_handle_return_plan(call)
                }
                (ValueReturnStrategy::CallbackHandle, ValueReturnMethod::DirectReturn) => {
                    JavaReturnPlan {
                        native_return_type: self.java_type(ty),
                        render: JavaReturnRender::Direct,
                    }
                }
                _ => JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                },
            },
        }
    }

    fn java_c_style_enum_return_plan(&self, ty: &TypeExpr, call: &AbiCall) -> JavaReturnPlan {
        match (ty, call.returns.transport.as_ref()) {
            (
                TypeExpr::Enum(enum_id),
                Some(Transport::Scalar(ScalarOrigin::CStyleEnum { tag_type, .. })),
            ) => JavaReturnPlan {
                native_return_type: mappings::java_type(*tag_type).to_string(),
                render: JavaReturnRender::CStyleEnum {
                    class_name: NamingConvention::class_name(enum_id.as_str()),
                },
            },
            _ => JavaReturnPlan {
                native_return_type: self.java_type(ty),
                render: JavaReturnRender::Direct,
            },
        }
    }

    fn java_decode_return_plan(&self, ty: &TypeExpr, call: &AbiCall) -> JavaReturnPlan {
        let decode_expr = match ty {
            TypeExpr::String => "reader.readString()".to_string(),
            TypeExpr::Option(_) => match &call.returns.decode_ops {
                Some(decode_seq) => super::emit::emit_reader_read(decode_seq),
                None => panic!(
                    "unsupported direct Option return transport for Java backend: {:?}",
                    ty
                ),
            },
            TypeExpr::Record(id) => {
                format!(
                    "{}.decode(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            TypeExpr::Enum(id) => {
                format!(
                    "{}.decode(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            TypeExpr::Vec(_) => match &call.returns.decode_ops {
                Some(decode_seq) => super::emit::emit_reader_read(decode_seq),
                None => {
                    return JavaReturnPlan {
                        native_return_type: "void".to_string(),
                        render: JavaReturnRender::Void,
                    };
                }
            },
            TypeExpr::Bytes => "_buf != null ? _buf : new byte[0]".to_string(),
            _ => {
                return JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                };
            }
        };
        JavaReturnPlan {
            native_return_type: "byte[]".to_string(),
            render: JavaReturnRender::Decode { decode_expr },
        }
    }

    fn java_direct_vec_return_plan(&self, ty: &TypeExpr, call: &AbiCall) -> JavaReturnPlan {
        match ty {
            TypeExpr::Bytes => JavaReturnPlan {
                native_return_type: "byte[]".to_string(),
                render: JavaReturnRender::Decode {
                    decode_expr: "_buf != null ? _buf : new byte[0]".to_string(),
                },
            },
            TypeExpr::Vec(inner) => JavaReturnPlan {
                native_return_type: "byte[]".to_string(),
                render: JavaReturnRender::Decode {
                    decode_expr: self
                        .vec_buffer_decode_expr(inner, call.returns.transport.as_ref()),
                },
            },
            _ => self.java_decode_return_plan(ty, call),
        }
    }

    fn java_handle_return_plan(&self, call: &AbiCall) -> JavaReturnPlan {
        match call.returns.transport.as_ref() {
            Some(Transport::Handle { class_id, nullable }) => JavaReturnPlan {
                native_return_type: "long".to_string(),
                render: JavaReturnRender::Handle {
                    class_name: NamingConvention::class_name(class_id.as_str()),
                    nullable: *nullable,
                },
            },
            _ => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
        }
    }

    fn async_call_from_mode(&self, call: &AbiCall, returns: &ReturnDef) -> Option<JavaAsyncCall> {
        let async_abi = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => return None,
        };
        let result_call = AbiCall {
            returns: async_abi.result.clone(),
            ..call.clone()
        };
        let complete_return_plan = self.return_plan(returns, &result_call);
        Some(JavaAsyncCall {
            poll: async_abi.poll.as_str().to_string(),
            complete: async_abi.complete.as_str().to_string(),
            cancel: async_abi.cancel.as_str().to_string(),
            free: async_abi.free.as_str().to_string(),
            complete_return_plan,
        })
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| matches!(&c.id, CallId::Function(id) if id == &func.id))
            .expect("abi call not found for function")
    }

    fn abi_call_for_constructor(&self, class: &ClassDef, index: usize) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| {
                c.id == CallId::Constructor {
                    class_id: class.id.clone(),
                    index,
                }
            })
            .expect("abi call not found for constructor")
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| {
                c.id == CallId::Method {
                    class_id: class.id.clone(),
                    method_id: method.id.clone(),
                }
            })
            .expect("abi call not found for method")
    }

    fn strip_receiver(call: &AbiCall) -> AbiCall {
        AbiCall {
            params: call
                .params
                .iter()
                .filter(|p| !Self::is_instance_receiver(p))
                .cloned()
                .collect(),
            ..call.clone()
        }
    }

    fn is_instance_receiver(param: &AbiParam) -> bool {
        param.name.as_str() == "self"
            && matches!(
                param.role,
                ParamRole::Input {
                    transport: Transport::Handle { .. },
                    ..
                }
            )
    }

    fn lower_class(&self, class: &ClassDef) -> JavaClass {
        let class_name = NamingConvention::class_name(class.id.as_str());
        let ffi_free = boltffi_ffi_rules::naming::class_ffi_free(class.id.as_str()).to_string();

        let constructors = self.resolve_constructor_collisions(
            class
                .constructors
                .iter()
                .enumerate()
                .map(|(index, ctor)| self.lower_constructor(class, ctor, index))
                .collect(),
        );

        let methods = class
            .methods
            .iter()
            .filter(|m| self.is_supported_method(m))
            .map(|m| self.lower_class_method(class, m))
            .collect();

        JavaClass {
            class_name,
            ffi_free,
            constructors,
            methods,
        }
    }

    fn resolve_constructor_collisions(
        &self,
        constructors: Vec<JavaConstructor>,
    ) -> Vec<JavaConstructor> {
        let preferred_indices = constructors
            .iter()
            .enumerate()
            .filter(|(_, constructor)| !constructor.is_factory())
            .fold(
                HashMap::<Vec<String>, usize>::new(),
                |mut indices, (index, constructor)| {
                    let signature = Self::constructor_signature(constructor);
                    match indices.get(&signature).copied() {
                        Some(existing_index)
                            if Self::prefers_constructor_surface(
                                constructor,
                                &constructors[existing_index],
                            ) =>
                        {
                            indices.insert(signature, index);
                        }
                        None => {
                            indices.insert(signature, index);
                        }
                        Some(_) => {}
                    }
                    indices
                },
            );

        constructors
            .into_iter()
            .enumerate()
            .map(|(index, mut constructor)| {
                if constructor.is_factory() {
                    return constructor;
                }

                let signature = Self::constructor_signature(&constructor);
                if preferred_indices.get(&signature).copied() != Some(index) {
                    constructor.kind = JavaConstructorKind::Factory;
                }
                constructor
            })
            .collect()
    }

    fn constructor_signature(constructor: &JavaConstructor) -> Vec<String> {
        constructor
            .params
            .iter()
            .map(|param| param.java_type.clone())
            .collect()
    }

    fn prefers_constructor_surface(candidate: &JavaConstructor, current: &JavaConstructor) -> bool {
        Self::constructor_priority(candidate) < Self::constructor_priority(current)
    }

    fn constructor_priority(constructor: &JavaConstructor) -> usize {
        match (constructor.kind, constructor.is_fallible) {
            (JavaConstructorKind::Primary, _) => 0,
            (JavaConstructorKind::Secondary, false) => 1,
            (JavaConstructorKind::Secondary, true) => 2,
            (JavaConstructorKind::Factory, _) => 3,
        }
    }

    fn is_supported_method(&self, method: &MethodDef) -> bool {
        let params_ok = method
            .params
            .iter()
            .all(|p| self.is_supported_type(&p.type_expr));
        let return_ok = match &method.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_method_return_type(ty),
            ReturnDef::Result { ok, err } => {
                self.is_supported_result_type(ok) && self.is_supported_result_type(err)
            }
        };
        params_ok && return_ok
    }

    fn is_supported_method_return_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Handle(_) => true,
            _ => self.is_supported_type(ty),
        }
    }

    fn lower_constructor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        index: usize,
    ) -> JavaConstructor {
        let call = self.abi_call_for_constructor(class, index);
        let wire_writers = self.wire_writers_for_params(call);

        let (kind, name) = match ctor {
            ConstructorDef::Default { .. } => (JavaConstructorKind::Primary, String::new()),
            ConstructorDef::NamedFactory { name, .. } => (
                JavaConstructorKind::Factory,
                NamingConvention::method_name(name.as_str()),
            ),
            ConstructorDef::NamedInit { name, .. } => (
                JavaConstructorKind::Secondary,
                NamingConvention::method_name(name.as_str()),
            ),
        };

        let params: Vec<JavaParam> = ctor
            .params()
            .iter()
            .enumerate()
            .map(|(param_index, param_def)| {
                self.lower_param(
                    param_def.name.as_str(),
                    &param_def.type_expr,
                    param_index,
                    call,
                    &wire_writers,
                )
            })
            .collect();

        JavaConstructor {
            kind,
            name,
            is_fallible: ctor.is_fallible(),
            params,
            ffi_name: call.symbol.as_str().to_string(),
            wire_writers,
        }
    }

    fn lower_class_method(&self, class: &ClassDef, method: &MethodDef) -> JavaClassMethod {
        let raw_call = self.abi_call_for_method(class, method);
        let call = Self::strip_receiver(raw_call);
        let wire_writers = self.wire_writers_for_params(&call);
        let is_static = method.receiver == Receiver::Static;

        let params: Vec<JavaParam> = method
            .params
            .iter()
            .enumerate()
            .map(|(param_index, param_def)| {
                self.lower_param(
                    param_def.name.as_str(),
                    &param_def.type_expr,
                    param_index,
                    &call,
                    &wire_writers,
                )
            })
            .collect();

        let async_call = self.async_call_from_mode(&call, &method.returns);
        let return_plan = match &async_call {
            Some(async_call) => async_call.complete_return_plan.clone(),
            None => self.return_plan(&method.returns, &call),
        };

        JavaClassMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            is_static,
            params,
            return_type: self.return_java_type(&method.returns),
            return_plan,
            wire_writers,
            async_call,
        }
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
            TypeExpr::Bytes => "byte[]".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Handle(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Callback(id) => self.callback_java_type(id),
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
            TypeExpr::Bytes => "byte[]".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Handle(id) => NamingConvention::class_name(id.as_str()),
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
        let kind = if enumeration.is_error {
            JavaEnumKind::Error
        } else if abi_enum.is_c_style {
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
            .enumerate()
            .map(|(ordinal, variant)| {
                self.lower_enum_variant(abi_enum, variant, ordinal, kind, &variant_names)
            })
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
        abi_enum: &AbiEnum,
        variant: &AbiEnumVariant,
        ordinal: usize,
        kind: JavaEnumKind,
        sibling_names: &HashSet<String>,
    ) -> JavaEnumVariant {
        let name = match kind {
            JavaEnumKind::CStyle | JavaEnumKind::Error => {
                NamingConvention::enum_constant_name(variant.name.as_str())
            }
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
            tag: self.java_enum_variant_tag(abi_enum, kind, ordinal, variant.discriminant),
            fields,
        }
    }

    fn java_enum_variant_tag(
        &self,
        abi_enum: &AbiEnum,
        kind: JavaEnumKind,
        ordinal: usize,
        discriminant: i128,
    ) -> i128 {
        match kind {
            JavaEnumKind::CStyle => discriminant,
            JavaEnumKind::Error | JavaEnumKind::SealedInterface | JavaEnumKind::AbstractClass => {
                match abi_enum.codec_tag_strategy {
                    EnumTagStrategy::Discriminant => discriminant,
                    EnumTagStrategy::OrdinalIndex => {
                        abi_enum.resolve_codec_tag(ordinal, discriminant)
                    }
                }
            }
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
            SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
                value: Self::prefix_value(value, binding),
                owner: owner.clone(),
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

    fn lower_closure(&self, callback: &CallbackTraitDef) -> JavaClosureInterface {
        let method = callback
            .methods
            .first()
            .expect("closure must have a method");
        let signature_id = callback
            .id
            .as_str()
            .strip_prefix("__Closure_")
            .unwrap_or(callback.id.as_str());
        let interface_name = format!("Closure{}", signature_id);
        let params = method
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let java_type = self.java_type(&param.type_expr);
                let jni_type = self.closure_jni_type(&param.type_expr);
                let name = format!("p{}", index);
                let jni_decode_expr = self.closure_param_decode(&param.type_expr, &name);
                JavaClosureParam {
                    name,
                    java_type,
                    jni_type,
                    jni_decode_expr,
                }
            })
            .collect();
        let return_ty = match &method.returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(ty),
            ReturnDef::Result { ok, .. } => Some(ok),
        };
        let return_type = return_ty.map(|ty| self.java_type(ty));
        let (jni_return_type, return_to_jni_expr) = match return_ty {
            None => (None, String::new()),
            Some(ty) => self.closure_return_jni_info(ty),
        };
        JavaClosureInterface {
            interface_name,
            callback_id: callback.id.as_str().to_string(),
            params,
            return_type,
            jni_return_type,
            return_to_jni_expr,
        }
    }

    fn closure_return_jni_info(&self, ty: &TypeExpr) -> (Option<String>, String) {
        match ty {
            TypeExpr::Primitive(p) => (Some(mappings::java_type(*p).to_string()), String::new()),
            TypeExpr::String => (
                Some("byte[]".to_string()),
                ".getBytes(java.nio.charset.StandardCharsets.UTF_8)".to_string(),
            ),
            TypeExpr::Record(_) => (
                Some("byte[]".to_string()),
                ".wireEncodeToBytes()".to_string(),
            ),
            TypeExpr::Enum(id) => {
                let enum_def = self.ffi.catalog.resolve_enum(id);
                let is_c_style = enum_def
                    .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
                    .unwrap_or(false);
                if is_c_style {
                    let tag_type = enum_def
                        .and_then(|e| match &e.repr {
                            EnumRepr::CStyle { tag_type, .. } => Some(*tag_type),
                            _ => None,
                        })
                        .unwrap_or(PrimitiveType::I32);
                    (
                        Some(mappings::java_type(tag_type).to_string()),
                        ".value".to_string(),
                    )
                } else {
                    (Some("byte[]".to_string()), ".encode()".to_string())
                }
            }
            _ => (Some("long".to_string()), String::new()),
        }
    }

    fn closure_param_decode(&self, ty: &TypeExpr, name: &str) -> String {
        match ty {
            TypeExpr::Primitive(_) => name.to_string(),
            TypeExpr::String => format!("WireReader.stringFromBuffer({})", name),
            TypeExpr::Record(id) => format!(
                "{}.decode(WireReader.fromBuffer({}))",
                NamingConvention::class_name(id.as_str()),
                name
            ),
            TypeExpr::Enum(id) => format!(
                "{}.fromValue({})",
                NamingConvention::class_name(id.as_str()),
                name
            ),
            _ => name.to_string(),
        }
    }

    fn closure_jni_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String | TypeExpr::Record(_) => "java.nio.ByteBuffer".to_string(),
            TypeExpr::Enum(id) => {
                let enum_def = self.ffi.catalog.resolve_enum(id);
                match enum_def.and_then(|e| match &e.repr {
                    EnumRepr::CStyle { tag_type, .. } => Some(*tag_type),
                    _ => None,
                }) {
                    Some(tag) => mappings::java_type(tag).to_string(),
                    None => "java.nio.ByteBuffer".to_string(),
                }
            }
            _ => "long".to_string(),
        }
    }

    fn lower_callback_trait(&self, callback: &CallbackTraitDef) -> JavaCallbackTrait {
        let interface_name = NamingConvention::class_name(callback.id.as_str());
        let abi_callback = self.abi_callback_for(&callback.id);
        let methods = callback
            .methods
            .iter()
            .filter(|method| !method.is_async)
            .filter_map(|method| {
                let abi_method = abi_callback
                    .as_ref()
                    .and_then(|ac| ac.methods.iter().find(|m| m.id == method.id));
                abi_method.map(|abi_method| self.lower_callback_method(method, abi_method))
            })
            .collect();
        JavaCallbackTrait {
            interface_name,
            callback_id: callback.id.as_str().to_string(),
            methods,
        }
    }

    fn lower_callback_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JavaCallbackMethod {
        let params = method
            .params
            .iter()
            .map(|param| {
                let java_type = self.java_type(&param.type_expr);
                let abi_param = abi_method
                    .params
                    .iter()
                    .find(|p| p.name.as_str() == param.name.as_str());
                let jni_type = abi_param
                    .map(|p| self.callback_param_jni_type(&param.type_expr, p))
                    .unwrap_or_else(|| java_type.clone());
                let decode_expr =
                    self.callback_param_decode_expr(&param.type_expr, param.name.as_str());
                JavaCallbackParam {
                    name: NamingConvention::field_name(param.name.as_str()),
                    java_type,
                    jni_type,
                    decode_expr,
                }
            })
            .collect();
        let return_info = match &method.returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(self.callback_return_info(ty)),
            ReturnDef::Result { ok, .. } => Some(self.callback_return_info(ok)),
        };
        JavaCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: boltffi_ffi_rules::naming::vtable_field_name(method.id.as_str())
                .into_string(),
            params,
            return_info,
        }
    }

    fn callback_param_jni_type(&self, ty: &TypeExpr, _abi_param: &AbiParam) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String => "java.nio.ByteBuffer".to_string(),
            TypeExpr::Enum(id) => {
                let enum_def = self.ffi.catalog.resolve_enum(id);
                match enum_def.and_then(|e| match &e.repr {
                    EnumRepr::CStyle { tag_type, .. } => Some(*tag_type),
                    _ => None,
                }) {
                    Some(tag) => mappings::java_type(tag).to_string(),
                    None => "java.nio.ByteBuffer".to_string(),
                }
            }
            TypeExpr::Record(_) => "java.nio.ByteBuffer".to_string(),
            TypeExpr::Vec(_) => "java.nio.ByteBuffer".to_string(),
            _ => "long".to_string(),
        }
    }

    fn callback_param_decode_expr(&self, ty: &TypeExpr, name: &str) -> String {
        let field_name = NamingConvention::field_name(name);
        match ty {
            TypeExpr::Primitive(_) => field_name,
            TypeExpr::String => format!("WireReader.stringFromBuffer({})", field_name),
            TypeExpr::Record(id) => format!(
                "{}.decode(WireReader.fromBuffer({}))",
                NamingConvention::class_name(id.as_str()),
                field_name
            ),
            TypeExpr::Enum(id) => {
                let enum_def = self.ffi.catalog.resolve_enum(id);
                let is_c_style = enum_def
                    .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
                    .unwrap_or(false);
                if is_c_style {
                    format!(
                        "{}.fromValue({})",
                        NamingConvention::class_name(id.as_str()),
                        field_name
                    )
                } else {
                    format!(
                        "{}.decode(new WireReader({}))",
                        NamingConvention::class_name(id.as_str()),
                        field_name
                    )
                }
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => {
                    let read_method = match p {
                        PrimitiveType::Bool => "readBooleanArray",
                        PrimitiveType::I8 | PrimitiveType::U8 => "readByteArray",
                        PrimitiveType::I16 | PrimitiveType::U16 => "readShortArray",
                        PrimitiveType::I32 | PrimitiveType::U32 => "readIntArray",
                        PrimitiveType::I64
                        | PrimitiveType::U64
                        | PrimitiveType::ISize
                        | PrimitiveType::USize => "readLongArray",
                        PrimitiveType::F32 => "readFloatArray",
                        PrimitiveType::F64 => "readDoubleArray",
                    };
                    format!("WireReader.fromBuffer({}).{}()", field_name, read_method)
                }
                _ => format!("WireReader.fromBuffer({}).readList(i -> null)", field_name),
            },
            _ => field_name,
        }
    }

    fn callback_return_info(&self, ty: &TypeExpr) -> JavaCallbackReturn {
        let no_wrap = String::new();
        match ty {
            TypeExpr::Primitive(p) => JavaCallbackReturn {
                java_type: mappings::java_type(*p).to_string(),
                jni_type: mappings::java_type(*p).to_string(),
                default_value: mappings::java_default_value(*p).to_string(),
                to_jni_expr: String::new(),
                wrap_prefix: no_wrap,
            },
            TypeExpr::String => JavaCallbackReturn {
                java_type: "String".to_string(),
                jni_type: "byte[]".to_string(),
                default_value: "new byte[0]".to_string(),
                to_jni_expr: ".getBytes(java.nio.charset.StandardCharsets.UTF_8)".to_string(),
                wrap_prefix: no_wrap,
            },
            TypeExpr::Record(id) => JavaCallbackReturn {
                java_type: NamingConvention::class_name(id.as_str()),
                jni_type: "byte[]".to_string(),
                default_value: "new byte[0]".to_string(),
                to_jni_expr: ".wireEncodeToBytes()".to_string(),
                wrap_prefix: no_wrap,
            },
            TypeExpr::Enum(id) => {
                let enum_def = self.ffi.catalog.resolve_enum(id);
                let tag_type = enum_def.and_then(|e| match &e.repr {
                    EnumRepr::CStyle { tag_type, .. } => Some(*tag_type),
                    _ => None,
                });
                match tag_type {
                    Some(tag) => JavaCallbackReturn {
                        java_type: NamingConvention::class_name(id.as_str()),
                        jni_type: mappings::java_type(tag).to_string(),
                        default_value: mappings::java_default_value(tag).to_string(),
                        to_jni_expr: ".value".to_string(),
                        wrap_prefix: no_wrap,
                    },
                    None => JavaCallbackReturn {
                        java_type: NamingConvention::class_name(id.as_str()),
                        jni_type: "byte[]".to_string(),
                        default_value: "new byte[0]".to_string(),
                        to_jni_expr: ".wireEncodeToBytes()".to_string(),
                        wrap_prefix: no_wrap,
                    },
                }
            }
            TypeExpr::Vec(_inner) => {
                let java_type = self.java_type(ty);
                JavaCallbackReturn {
                    java_type,
                    jni_type: "byte[]".to_string(),
                    default_value: "new byte[0]".to_string(),
                    to_jni_expr: ")".to_string(),
                    wrap_prefix: "WireReader.wireEncodeIntArray(".to_string(),
                }
            }
            TypeExpr::Handle(id) => JavaCallbackReturn {
                java_type: NamingConvention::class_name(id.as_str()),
                jni_type: "long".to_string(),
                default_value: "0L".to_string(),
                to_jni_expr: ".handle".to_string(),
                wrap_prefix: no_wrap,
            },
            _ => JavaCallbackReturn {
                java_type: "Object".to_string(),
                jni_type: "Object".to_string(),
                default_value: "null".to_string(),
                to_jni_expr: String::new(),
                wrap_prefix: no_wrap,
            },
        }
    }

    fn callback_java_type(&self, callback_id: &CallbackId) -> String {
        let callback = self.ffi.catalog.resolve_callback(callback_id);
        match callback {
            Some(cb) if matches!(cb.kind, CallbackKind::Closure) => {
                let signature_id = callback_id
                    .as_str()
                    .strip_prefix("__Closure_")
                    .unwrap_or(callback_id.as_str());
                format!("Closure{}", signature_id)
            }
            Some(_) => NamingConvention::class_name(callback_id.as_str()),
            None => "Object".to_string(),
        }
    }

    fn callback_bridge_name(&self, callback_id: &CallbackId) -> String {
        let callback = self.ffi.catalog.resolve_callback(callback_id);
        match callback {
            Some(cb) if matches!(cb.kind, CallbackKind::Closure) => {
                let signature_id = callback_id
                    .as_str()
                    .strip_prefix("__Closure_")
                    .unwrap_or(callback_id.as_str());
                format!("Closure{}Callbacks", signature_id)
            }
            Some(_) => format!(
                "{}Callbacks",
                NamingConvention::class_name(callback_id.as_str())
            ),
            None => "Object".to_string(),
        }
    }

    fn abi_callback_for(&self, callback_id: &CallbackId) -> Option<&AbiCallbackInvocation> {
        self.abi
            .callbacks
            .iter()
            .find(|cb| cb.callback_id == *callback_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CStyleVariant, ClassDef, ConstructorDef, DataVariant, EnumDef, EnumRepr, FieldDef,
        FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef,
        VariantPayload,
    };
    use crate::ir::ids::{
        ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId, VariantName,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use crate::render::java::JavaVersion;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn lower(contract: &FfiContract) -> JavaModule {
        lower_with_version(contract, JavaVersion::JAVA_8)
    }

    fn lower_with_version(contract: &FfiContract, version: JavaVersion) -> JavaModule {
        let abi = IrLowerer::new(contract).to_abi_contract();
        let options = JavaOptions {
            library_name: None,
            min_java_version: version,
        };
        JavaLowerer::new(
            contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            options,
        )
        .module()
    }

    fn record_def(id: &str, fields: Vec<FieldDef>) -> RecordDef {
        RecordDef {
            is_repr_c: true,
            id: RecordId::new(id),
            fields,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn field(name: &str, ty: TypeExpr) -> FieldDef {
        FieldDef {
            name: FieldName::new(name),
            type_expr: ty,
            doc: None,
            default: None,
        }
    }

    fn param(name: &str, ty: TypeExpr) -> ParamDef {
        ParamDef {
            name: ParamName::new(name),
            type_expr: ty,
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    fn function(id: &str, params: Vec<ParamDef>, returns: ReturnDef) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(id),
            params,
            returns,
            is_async: false,
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn primitive_type_mapping() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "AllPrimitives",
            vec![
                field("a", TypeExpr::Primitive(PrimitiveType::Bool)),
                field("b", TypeExpr::Primitive(PrimitiveType::I8)),
                field("c", TypeExpr::Primitive(PrimitiveType::U8)),
                field("d", TypeExpr::Primitive(PrimitiveType::I16)),
                field("e", TypeExpr::Primitive(PrimitiveType::U16)),
                field("f", TypeExpr::Primitive(PrimitiveType::I32)),
                field("g", TypeExpr::Primitive(PrimitiveType::U32)),
                field("h", TypeExpr::Primitive(PrimitiveType::I64)),
                field("i", TypeExpr::Primitive(PrimitiveType::U64)),
                field("j", TypeExpr::Primitive(PrimitiveType::F32)),
                field("k", TypeExpr::Primitive(PrimitiveType::F64)),
                field("l", TypeExpr::Primitive(PrimitiveType::ISize)),
                field("m", TypeExpr::Primitive(PrimitiveType::USize)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].java_type, "boolean");
        assert_eq!(record.fields[1].java_type, "byte");
        assert_eq!(record.fields[2].java_type, "byte");
        assert_eq!(record.fields[3].java_type, "short");
        assert_eq!(record.fields[4].java_type, "short");
        assert_eq!(record.fields[5].java_type, "int");
        assert_eq!(record.fields[6].java_type, "int");
        assert_eq!(record.fields[7].java_type, "long");
        assert_eq!(record.fields[8].java_type, "long");
        assert_eq!(record.fields[9].java_type, "float");
        assert_eq!(record.fields[10].java_type, "double");
        assert_eq!(record.fields[11].java_type, "long");
        assert_eq!(record.fields[12].java_type, "long");
    }

    #[test]
    fn string_field_type() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Named", vec![field("name", TypeExpr::String)]));

        let module = lower(&contract);
        assert_eq!(module.records[0].fields[0].java_type, "String");
    }

    #[test]
    fn bytes_field_type() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Buffer", vec![field("data", TypeExpr::Bytes)]));

        let module = lower(&contract);
        assert_eq!(module.records[0].fields[0].java_type, "byte[]");
    }

    #[test]
    fn option_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "MaybeValue",
            vec![
                field(
                    "count",
                    TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                ),
                field("label", TypeExpr::Option(Box::new(TypeExpr::String))),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert_eq!(record.fields[0].java_type, "java.util.Optional<Integer>");
        assert_eq!(record.fields[1].java_type, "java.util.Optional<String>");
    }

    #[test]
    fn vec_primitive_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Buffers",
            vec![
                field(
                    "ints",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                ),
                field(
                    "longs",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I64))),
                ),
                field(
                    "bytes",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
                ),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert_eq!(record.fields[0].java_type, "int[]");
        assert_eq!(record.fields[1].java_type, "long[]");
        assert_eq!(record.fields[2].java_type, "byte[]");
    }

    #[test]
    fn vec_string_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Tags",
            vec![field("values", TypeExpr::Vec(Box::new(TypeExpr::String)))],
        ));

        let module = lower(&contract);
        assert_eq!(
            module.records[0].fields[0].java_type,
            "java.util.List<String>"
        );
    }

    #[test]
    fn record_field_names_are_camel_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Config",
            vec![
                field("max_connections", TypeExpr::Primitive(PrimitiveType::I32)),
                field("timeout_ms", TypeExpr::Primitive(PrimitiveType::U64)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert_eq!(record.fields[0].name, "maxConnections");
        assert_eq!(record.fields[1].name, "timeoutMs");
    }

    #[test]
    fn record_class_name_is_pascal_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "my_point",
            vec![field("x", TypeExpr::Primitive(PrimitiveType::F64))],
        ));

        let module = lower(&contract);
        assert_eq!(module.records[0].class_name, "MyPoint");
    }

    #[test]
    fn blittable_record_detected() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Point",
            vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert!(record.is_blittable());
        let layout = record.blittable_layout.as_ref().unwrap();
        assert_eq!(layout.struct_size, 16);
        assert_eq!(layout.fields.len(), 2);
    }

    #[test]
    fn non_blittable_record_with_string() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "User",
            vec![
                field("id", TypeExpr::Primitive(PrimitiveType::I64)),
                field("name", TypeExpr::String),
            ],
        ));

        let module = lower(&contract);
        assert!(!module.records[0].is_blittable());
    }

    #[test]
    fn record_shape_classic_on_java8() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Simple",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_8);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_native_on_java17_without_primitive_arrays() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Simple",
            vec![
                field("value", TypeExpr::Primitive(PrimitiveType::I32)),
                field("name", TypeExpr::String),
            ],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::NativeRecord);
    }

    #[test]
    fn record_shape_classic_on_java17_with_primitive_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "WithArray",
            vec![field(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_classic_on_java17_with_bytes() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "WithBytes",
            vec![field("data", TypeExpr::Bytes)],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_classic_on_java17_with_nested_primitive_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Nested",
            vec![field(
                "matrix",
                TypeExpr::Option(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_native_on_java17_with_vec_of_strings() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "WithStringList",
            vec![field("tags", TypeExpr::Vec(Box::new(TypeExpr::String)))],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::NativeRecord);
    }

    #[test]
    fn bytes_equality_uses_arrays_equals() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Buffer", vec![field("data", TypeExpr::Bytes)]));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("java.util.Arrays.equals("),
            "expected Arrays.equals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn bytes_hash_uses_arrays_hash_code() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Buffer", vec![field("data", TypeExpr::Bytes)]));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.hash_expr.contains("java.util.Arrays.hashCode("),
            "expected Arrays.hashCode, got: {}",
            field.hash_expr
        );
    }

    #[test]
    fn primitive_array_equality_uses_arrays_equals() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Scores",
            vec![field(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("java.util.Arrays.equals("),
            "expected Arrays.equals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn string_equality_uses_objects_equals() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Named", vec![field("name", TypeExpr::String)]));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("java.util.Objects.equals("),
            "expected Objects.equals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn primitive_equality_uses_direct_comparison() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Counter",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("=="),
            "expected ==, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn float_equality_uses_compare() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Coords",
            vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F32)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert!(record.fields[0].equals_expr.contains("Float.compare("));
        assert!(record.fields[1].equals_expr.contains("Double.compare("));
    }

    #[test]
    fn option_equality_checks_presence() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "MaybeValue",
            vec![field(
                "value",
                TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("isPresent()"),
            "expected isPresent check, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn nested_vec_equality_uses_list_equals() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Matrix",
            vec![field(
                "rows",
                TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("WireWriter.listEquals("),
            "expected WireWriter.listEquals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn c_style_enum_lowering() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    CStyleVariant {
                        name: VariantName::new("Active"),
                        discriminant: 0,
                        doc: None,
                    },
                    CStyleVariant {
                        name: VariantName::new("Inactive"),
                        discriminant: 1,
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        assert_eq!(module.enums.len(), 1);
        let e = &module.enums[0];
        assert_eq!(e.class_name, "Status");
        assert!(e.is_c_style());
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name, "ACTIVE");
        assert_eq!(e.variants[1].name, "INACTIVE");
    }

    #[test]
    fn data_enum_abstract_on_java8() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    DataVariant {
                        name: VariantName::new("Circle"),
                        discriminant: 0,
                        payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(
                            PrimitiveType::F64,
                        )]),
                        doc: None,
                    },
                    DataVariant {
                        name: VariantName::new("Square"),
                        discriminant: 1,
                        payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(
                            PrimitiveType::F64,
                        )]),
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_8);
        assert!(module.enums[0].is_abstract());
    }

    #[test]
    fn data_enum_sealed_on_java17() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Circle"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(PrimitiveType::F64)]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert!(module.enums[0].is_sealed());
    }

    #[test]
    fn data_enum_falls_back_to_abstract_with_primitive_array_field() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Payload"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Binary"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::Vec(Box::new(
                        TypeExpr::Primitive(PrimitiveType::U8),
                    ))]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert!(
            module.enums[0].is_abstract(),
            "should fall back to abstract due to primitive array field"
        );
    }

    #[test]
    fn data_enum_variant_names_are_pascal_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Value"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("some_text"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        assert_eq!(module.enums[0].variants[0].name, "SomeText");
    }

    #[test]
    fn function_primitive_return_is_direct() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_count",
            vec![],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "int");
        assert!(func.return_plan.is_direct());
    }

    #[test]
    fn function_void_return() {
        let mut contract = empty_contract();
        contract
            .functions
            .push(function("do_nothing", vec![], ReturnDef::Void));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "void");
        assert!(func.return_plan.is_void());
    }

    #[test]
    fn function_string_return_is_wire_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_name",
            vec![],
            ReturnDef::Value(TypeExpr::String),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "String");
        assert!(func.return_plan.is_decode());
        assert!(func.return_plan.decode_expr().contains("readString"));
    }

    #[test]
    fn function_bytes_return_is_buffer_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_data",
            vec![],
            ReturnDef::Value(TypeExpr::Bytes),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "byte[]");
        assert!(func.return_plan.is_decode());
        assert!(
            func.return_plan.decode_expr().contains("new byte[0]"),
            "expected null-safe buffer decode, got: {}",
            func.return_plan.decode_expr()
        );
    }

    #[test]
    fn function_string_param_converts_to_utf8_bytes() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "greet",
            vec![param("name", TypeExpr::String)],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let p = &module.functions[0].params[0];
        assert_eq!(p.java_type, "String");
        assert_eq!(p.native_type, "byte[]");
        assert!(
            p.native_expr.contains("getBytes"),
            "expected UTF-8 conversion, got: {}",
            p.native_expr
        );
    }

    #[test]
    fn function_bytes_param_passes_directly() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "process",
            vec![param("data", TypeExpr::Bytes)],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let p = &module.functions[0].params[0];
        assert_eq!(p.java_type, "byte[]");
        assert_eq!(p.native_type, "byte[]");
        assert_eq!(p.native_expr, "data");
    }

    #[test]
    fn function_record_param_uses_wire_writer() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Point",
            vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.functions.push(function(
            "translate",
            vec![param("point", TypeExpr::Record(RecordId::new("Point")))],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        let p = &func.params[0];
        assert_eq!(p.native_type, "ByteBuffer");
        assert!(!func.wire_writers.is_empty());
    }

    #[test]
    fn function_option_param_uses_wire_writer() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "maybe",
            vec![param(
                "value",
                TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.params[0].native_type, "ByteBuffer");
        assert!(!func.wire_writers.is_empty());
    }

    #[test]
    fn function_option_return_is_wire_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "find",
            vec![],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "java.util.Optional<Integer>");
        assert!(func.return_plan.is_decode());
        assert!(func.return_plan.decode_expr().contains("Optional"));
    }

    #[test]
    fn function_name_is_camel_case() {
        let mut contract = empty_contract();
        contract
            .functions
            .push(function("get_max_value", vec![], ReturnDef::Void));

        let module = lower(&contract);
        assert_eq!(module.functions[0].name, "getMaxValue");
    }

    #[test]
    fn c_style_enum_return_is_direct_decode() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![CStyleVariant {
                    name: VariantName::new("Ok"),
                    discriminant: 0,
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function(
            "get_status",
            vec![],
            ReturnDef::Value(TypeExpr::Enum(EnumId::new("Status"))),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert!(func.return_plan.is_c_style_enum());
        assert_eq!(func.return_plan.c_style_enum_class(), "Status");
    }

    #[test]
    fn record_with_record_field() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Inner",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));
        contract.catalog.insert_record(record_def(
            "Outer",
            vec![field("inner", TypeExpr::Record(RecordId::new("Inner")))],
        ));

        let module = lower(&contract);
        let outer = module
            .records
            .iter()
            .find(|r| r.class_name == "Outer")
            .unwrap();
        assert_eq!(outer.fields[0].java_type, "Inner");
    }

    #[test]
    fn vec_of_records_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Item",
            vec![field("id", TypeExpr::Primitive(PrimitiveType::I32))],
        ));
        contract.catalog.insert_record(record_def(
            "Container",
            vec![field(
                "items",
                TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("Item")))),
            )],
        ));

        let module = lower(&contract);
        let container = module
            .records
            .iter()
            .find(|r| r.class_name == "Container")
            .unwrap();
        assert_eq!(container.fields[0].java_type, "java.util.List<Item>");
    }

    #[test]
    fn async_functions_are_included_with_async_call() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("slow_op"),
            params: vec![],
            returns: ReturnDef::Void,
            is_async: true,
            doc: None,
            deprecated: None,
        });
        contract
            .functions
            .push(function("fast_op", vec![], ReturnDef::Void));

        let module = lower(&contract);
        assert_eq!(module.functions.len(), 2);
        let slow = module
            .functions
            .iter()
            .find(|f| f.name == "slowOp")
            .unwrap();
        let fast = module
            .functions
            .iter()
            .find(|f| f.name == "fastOp")
            .unwrap();
        assert!(slow.is_async());
        assert!(!fast.is_async());
    }

    #[test]
    fn callback_params_are_supported() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "with_callback",
            vec![param(
                "cb",
                TypeExpr::Callback(crate::ir::ids::CallbackId::new("Foo")),
            )],
            ReturnDef::Void,
        ));
        contract
            .functions
            .push(function("simple", vec![], ReturnDef::Void));

        let module = lower(&contract);
        assert_eq!(module.functions.len(), 2);
    }

    #[test]
    fn module_metadata() {
        let contract = empty_contract();
        let module = lower(&contract);
        assert_eq!(module.package_name, "com.test");
        assert_eq!(module.class_name, "Test");
    }

    #[test]
    fn vec_primitive_return_is_buffer_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_scores",
            vec![],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "int[]");
        assert!(func.return_plan.is_decode());
    }

    #[test]
    fn data_enum_field_has_wire_expressions() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Value"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Text"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let variant = &module.enums[0].variants[0];
        let field = &variant.fields[0];
        assert!(!field.wire_decode_expr.is_empty());
        assert!(!field.wire_size_expr.is_empty());
        assert!(!field.wire_encode_expr.is_empty());
        assert!(!field.equals_expr.is_empty());
        assert!(!field.hash_expr.is_empty());
    }

    #[test]
    fn bytes_supported_in_function_params() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "echo",
            vec![param("data", TypeExpr::Bytes)],
            ReturnDef::Value(TypeExpr::Bytes),
        ));

        let module = lower(&contract);
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn option_of_bytes_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "MaybeBuffer",
            vec![field("data", TypeExpr::Option(Box::new(TypeExpr::Bytes)))],
        ));

        let module = lower(&contract);
        assert_eq!(
            module.records[0].fields[0].java_type,
            "java.util.Optional<byte[]>"
        );
    }

    fn class_def(id: &str, constructors: Vec<ConstructorDef>, methods: Vec<MethodDef>) -> ClassDef {
        ClassDef {
            id: ClassId::from(id),
            constructors,
            methods,
            streams: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn default_ctor(params: Vec<ParamDef>) -> ConstructorDef {
        ConstructorDef::Default {
            params,
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn fallible_default_ctor(params: Vec<ParamDef>) -> ConstructorDef {
        ConstructorDef::Default {
            params,
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn named_factory(name: &str) -> ConstructorDef {
        ConstructorDef::NamedFactory {
            name: name.into(),
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn named_init(name: &str, params: Vec<ParamDef>) -> ConstructorDef {
        let mut params_iter = params.into_iter();
        let first_param = params_iter
            .next()
            .expect("named init needs at least one param");
        ConstructorDef::NamedInit {
            name: name.into(),
            first_param,
            rest_params: params_iter.collect(),
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn fallible_named_init(name: &str, params: Vec<ParamDef>) -> ConstructorDef {
        let mut params_iter = params.into_iter();
        let first_param = params_iter
            .next()
            .expect("named init needs at least one param");
        ConstructorDef::NamedInit {
            name: name.into(),
            first_param,
            rest_params: params_iter.collect(),
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn instance_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::RefSelf,
            params,
            returns,
            is_async: false,
            doc: None,
            deprecated: None,
        }
    }

    fn static_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::Static,
            params,
            returns,
            is_async: false,
            doc: None,
            deprecated: None,
        }
    }

    fn param_def(name: &str, ty: TypeExpr) -> ParamDef {
        ParamDef {
            name: ParamName::from(name),
            type_expr: ty,
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    #[test]
    fn class_basic_default_constructor() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![param_def(
                "initial",
                TypeExpr::Primitive(PrimitiveType::I32),
            )])],
            vec![],
        ));

        let module = lower(&contract);
        assert_eq!(module.classes.len(), 1);

        let class = &module.classes[0];
        assert_eq!(class.class_name, "Counter");
        assert_eq!(class.constructors.len(), 1);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Primary);
        assert!(!class.constructors[0].is_fallible);
        assert_eq!(class.constructors[0].params.len(), 1);
        assert_eq!(class.constructors[0].params[0].name, "initial");
        assert_eq!(class.constructors[0].params[0].java_type, "int");
    }

    #[test]
    fn class_factory_constructor() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_def("inventory", vec![named_factory("empty")], vec![]));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.constructors.len(), 1);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Factory);
        assert_eq!(class.constructors[0].name, "empty");
        assert!(class.has_factory_constructors());
    }

    #[test]
    fn class_named_init_constructor() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "inventory",
            vec![named_init(
                "with_capacity",
                vec![param_def(
                    "capacity",
                    TypeExpr::Primitive(PrimitiveType::I32),
                )],
            )],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Secondary);
        assert_eq!(class.constructors[0].name, "withCapacity");
        assert_eq!(class.constructors[0].params.len(), 1);
        assert_eq!(class.constructors[0].params[0].name, "capacity");
    }

    #[test]
    fn class_fallible_constructor() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "connection",
            vec![fallible_default_ctor(vec![param_def(
                "url",
                TypeExpr::String,
            )])],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert!(class.constructors[0].is_fallible);
    }

    #[test]
    fn class_instance_method_returning_primitive() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "get_value",
                vec![],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 1);
        assert_eq!(class.methods[0].name, "getValue");
        assert!(!class.methods[0].is_static);
        assert_eq!(class.methods[0].return_type, "int");
    }

    #[test]
    fn class_static_method() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "math_utils",
            vec![default_ctor(vec![])],
            vec![static_method(
                "add",
                vec![
                    param_def("a", TypeExpr::Primitive(PrimitiveType::I32)),
                    param_def("b", TypeExpr::Primitive(PrimitiveType::I32)),
                ],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 1);
        assert!(class.methods[0].is_static);
        assert_eq!(class.methods[0].name, "add");
        assert_eq!(class.methods[0].params.len(), 2);
        assert!(class.has_static_methods());
    }

    #[test]
    fn class_void_method() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![])],
            vec![instance_method("increment", vec![], ReturnDef::Void)],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods[0].return_type, "void");
        assert!(class.methods[0].return_plan.is_void());
    }

    #[test]
    fn class_method_returning_string() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "person",
            vec![default_ctor(vec![param_def("name", TypeExpr::String)])],
            vec![instance_method(
                "get_name",
                vec![],
                ReturnDef::Value(TypeExpr::String),
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods[0].return_type, "String");
    }

    #[test]
    fn class_includes_async_methods_with_async_call() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "worker",
            vec![default_ctor(vec![])],
            vec![
                instance_method(
                    "sync_op",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ),
                MethodDef {
                    id: MethodId::from("async_op"),
                    receiver: Receiver::RefSelf,
                    params: vec![],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                    is_async: true,
                    doc: None,
                    deprecated: None,
                },
            ],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 2);
        let sync_method = class.methods.iter().find(|m| m.name == "syncOp").unwrap();
        let async_method = class.methods.iter().find(|m| m.name == "asyncOp").unwrap();
        assert!(!sync_method.is_async());
        assert!(async_method.is_async());
        let ac = async_method.async_call.as_ref().unwrap();
        assert!(!ac.poll.is_empty());
        assert!(!ac.complete.is_empty());
        assert!(!ac.cancel.is_empty());
        assert!(!ac.free.is_empty());
    }

    #[test]
    fn class_includes_result_methods() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "processor",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "process",
                vec![],
                ReturnDef::Result {
                    ok: TypeExpr::Primitive(PrimitiveType::I32),
                    err: TypeExpr::String,
                },
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 1);
        assert_eq!(class.methods[0].return_type, "int");
        assert!(class.methods[0].return_plan.is_result());
        assert!(class.methods[0].return_plan.result_err_is_string());
    }

    #[test]
    fn class_ffi_free_name() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_def("counter", vec![default_ctor(vec![])], vec![]));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert!(!class.ffi_free.is_empty());
        assert!(class.ffi_free.contains("counter"));
    }

    #[test]
    fn class_multiple_constructors() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "inventory",
            vec![
                default_ctor(vec![]),
                named_factory("empty"),
                named_init(
                    "with_capacity",
                    vec![param_def(
                        "capacity",
                        TypeExpr::Primitive(PrimitiveType::I32),
                    )],
                ),
            ],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.constructors.len(), 3);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Primary);
        assert_eq!(class.constructors[1].kind, JavaConstructorKind::Factory);
        assert_eq!(class.constructors[2].kind, JavaConstructorKind::Secondary);
    }

    #[test]
    fn class_constructor_signature_collision_demotes_fallible_named_init_to_factory() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "inventory",
            vec![
                named_init(
                    "with_capacity",
                    vec![param_def(
                        "capacity",
                        TypeExpr::Primitive(PrimitiveType::I32),
                    )],
                ),
                fallible_named_init(
                    "try_new",
                    vec![param_def(
                        "capacity",
                        TypeExpr::Primitive(PrimitiveType::I32),
                    )],
                ),
            ],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];

        assert_eq!(class.constructors.len(), 2);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Secondary);
        assert_eq!(class.constructors[0].name, "withCapacity");
        assert_eq!(class.constructors[1].kind, JavaConstructorKind::Factory);
        assert_eq!(class.constructors[1].name, "tryNew");
        assert!(class.has_factory_constructors());
    }

    #[test]
    fn class_has_wire_params_propagates() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "config",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![param_def(
                "config",
                TypeExpr::Record(RecordId::new("config")),
            )])],
            vec![],
        ));

        let module = lower(&contract);
        assert!(module.has_wire_params());
    }

    #[test]
    fn handle_return_strategy_fields() {
        let return_plan = JavaReturnPlan {
            native_return_type: "long".to_string(),
            render: JavaReturnRender::Handle {
                class_name: "Counter".to_string(),
                nullable: false,
            },
        };
        assert!(return_plan.is_handle());
        assert_eq!(return_plan.handle_class(), "Counter");
        assert!(!return_plan.handle_nullable());
        assert_eq!(return_plan.native_return_type, "long");
    }

    #[test]
    fn handle_return_strategy_nullable() {
        let return_plan = JavaReturnPlan {
            native_return_type: "long".to_string(),
            render: JavaReturnRender::Handle {
                class_name: "Counter".to_string(),
                nullable: true,
            },
        };
        assert!(return_plan.handle_nullable());
    }

    #[test]
    fn handle_type_in_method_return() {
        let mut contract = empty_contract();
        let target_class_id = ClassId::from("target");
        contract
            .catalog
            .insert_class(class_def("target", vec![default_ctor(vec![])], vec![]));
        contract.catalog.insert_class(class_def(
            "factory",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "create_target",
                vec![],
                ReturnDef::Value(TypeExpr::Handle(target_class_id)),
            )],
        ));

        let module = lower(&contract);
        let factory_class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Factory")
            .unwrap();
        assert_eq!(factory_class.methods.len(), 1);
        assert_eq!(factory_class.methods[0].return_type, "Target");
        assert!(factory_class.methods[0].return_plan.is_handle());
        assert_eq!(
            factory_class.methods[0].return_plan.handle_class(),
            "Target"
        );
    }

    #[test]
    fn async_function_has_poll_complete_cancel_free() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("fetch_data"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: true,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        assert!(func.is_async());
        let ac = func.async_call.as_ref().unwrap();
        assert!(ac.poll.contains("fetch_data"));
        assert!(ac.complete.contains("fetch_data"));
        assert!(ac.cancel.contains("fetch_data"));
        assert!(ac.free.contains("fetch_data"));
    }

    #[test]
    fn async_mode_is_virtual_thread_for_java21() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("op"),
            params: vec![],
            returns: ReturnDef::Void,
            is_async: true,
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_21);
        assert!(module.async_mode.is_virtual_thread());

        let module8 = lower_with_version(&contract, JavaVersion::JAVA_8);
        assert!(module8.async_mode.is_completable_future());
    }

    #[test]
    fn async_function_strategy_matches_return_type() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("fetch_data"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: true,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        let ac = func.async_call.as_ref().unwrap();
        assert!(ac.complete_return_plan.is_decode());
    }

    #[test]
    fn async_function_boxed_return_type_for_primitives() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("get_count"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            is_async: true,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "int");
        assert_eq!(func.boxed_return_type(), "Integer");
    }

    #[test]
    fn async_void_function_strategy() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("fire_and_forget"),
            params: vec![],
            returns: ReturnDef::Void,
            is_async: true,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        let ac = func.async_call.as_ref().unwrap();
        assert!(ac.complete_return_plan.is_void());
    }
}
