use std::collections::{HashMap, HashSet};

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, CallId, CallMode,
    ErrorTransport, OutputShape, StreamItemTransport,
};
use crate::ir::codec::VecLayout;
use crate::ir::contract::FfiContract;
use crate::ir::definitions::Receiver;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, CustomTypeDef,
    DefaultValue, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, RecordDef,
    ReturnDef, StreamDef, StreamMode, VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, FieldName, MethodId, ParamName, RecordId,
};
use crate::ir::ops::{
    FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape,
    WriteOp, WriteSeq, remap_root_in_seq,
};
use crate::ir::plan::AbiType;
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::ir::{FastOutputBinding, InputBinding, OutputBinding, ParamBinding};
use crate::render::TypeMappings;
use crate::render::kotlin::emit;
use crate::render::kotlin::plan::*;
use crate::render::kotlin::templates::{AsyncMethodTemplate, WireMethodTemplate};
use crate::render::kotlin::{
    FactoryStyle, KotlinApiStyle as KotlinInputApiStyle, KotlinOptions, NamingConvention,
};
use askama::Template;
use boltffi_ffi_rules::naming;

fn param_binding(param: &AbiParam) -> ParamBinding<'_> {
    param.param_binding()
}

fn call_output_binding(call: &AbiCall) -> OutputBinding<'_> {
    call.output_binding()
}

fn callback_output_binding(callback_method: &AbiCallbackMethod) -> OutputBinding<'_> {
    callback_method.output_shape.output_binding()
}

fn async_output_binding(async_call: &crate::ir::abi::AsyncCall) -> OutputBinding<'_> {
    async_call.result_shape.output_binding()
}

struct KotlinReturnMeta {
    is_unit: bool,
    is_direct: bool,
    cast: String,
}

pub struct KotlinLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: KotlinOptions,
    type_mappings: TypeMappings,
}

impl<'a> KotlinLowerer<'a> {
    pub fn new(
        contract: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: KotlinOptions,
    ) -> Self {
        Self {
            contract,
            abi,
            package_name,
            module_name,
            options,
            type_mappings: TypeMappings::new(),
        }
    }

    pub fn with_type_mappings(mut self, mappings: TypeMappings) -> Self {
        self.type_mappings = mappings;
        self
    }

    pub fn lower(&self) -> KotlinModule {
        let has_streams = self
            .contract
            .catalog
            .all_classes()
            .any(|class| !class.streams.is_empty());
        let preamble = self.lower_preamble(has_streams);
        let enums = self
            .contract
            .catalog
            .all_enums()
            .map(|e| self.lower_enum(e))
            .collect::<Vec<_>>();
        let data_enum_codecs = self
            .contract
            .catalog
            .all_enums()
            .filter(|e| self.should_generate_fixed_enum_codec(e))
            .map(|e| self.lower_data_enum_codec(e))
            .collect::<Vec<_>>();
        let records = self
            .contract
            .catalog
            .all_records()
            .map(|r| self.lower_record(r))
            .collect::<Vec<_>>();
        let record_readers = self.lower_record_readers();
        let record_writers = self.lower_record_writers();
        let closures = self.lower_closures();
        let functions = self
            .contract
            .functions
            .iter()
            .map(|function| self.lower_function(function))
            .collect::<Vec<_>>();
        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_class(class))
            .collect::<Vec<_>>();
        let callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .map(|c| self.lower_callback_trait(c))
            .collect::<Vec<_>>();
        let native = self.lower_native();

        KotlinModule {
            package_name: self.package_name.clone(),
            prefix: preamble.prefix,
            extra_imports: preamble.extra_imports,
            custom_types: preamble.custom_types,
            enums,
            data_enum_codecs,
            records,
            record_readers,
            record_writers,
            closures,
            functions,
            classes,
            callbacks,
            native,
            api_style: match self.options.api_style {
                KotlinInputApiStyle::TopLevel => KotlinApiStyle::TopLevel,
                KotlinInputApiStyle::ModuleObject => KotlinApiStyle::ModuleObject,
            },
            module_object_name: self.options.module_object_name.clone(),
            has_streams,
        }
    }

    fn lower_preamble(&self, has_streams: bool) -> KotlinPreamble {
        let extra_imports = self.collect_extra_imports(has_streams);
        let custom_types = self
            .contract
            .catalog
            .all_custom_types()
            .map(|custom| self.lower_custom_type(custom))
            .collect::<Vec<_>>();

        KotlinPreamble {
            prefix: naming::ffi_prefix().to_string(),
            extra_imports,
            custom_types,
            has_streams,
        }
    }

    fn collect_extra_imports(&self, has_streams: bool) -> Vec<String> {
        let mut imports = self
            .collect_builtin_ids()
            .into_iter()
            .filter_map(|id| self.builtin_import(&id))
            .collect::<Vec<_>>();
        let has_async_callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .any(|callback| callback.methods.iter().any(|method| method.is_async));
        let coroutine_imports = if has_async_callbacks || has_streams {
            vec![
                "kotlinx.coroutines.CoroutineScope".to_string(),
                "kotlinx.coroutines.Dispatchers".to_string(),
                "kotlinx.coroutines.SupervisorJob".to_string(),
                "kotlinx.coroutines.Job".to_string(),
                "kotlinx.coroutines.launch".to_string(),
            ]
        } else {
            Vec::new()
        };
        let stream_imports = if has_streams {
            vec![
                "java.util.concurrent.atomic.AtomicInteger".to_string(),
                "kotlinx.coroutines.channels.awaitClose".to_string(),
                "kotlinx.coroutines.flow.Flow".to_string(),
                "kotlinx.coroutines.flow.callbackFlow".to_string(),
            ]
        } else {
            Vec::new()
        };
        let builtin_imports = vec![
            "java.time.Duration".to_string(),
            "java.time.Instant".to_string(),
            "java.util.UUID".to_string(),
            "java.net.URI".to_string(),
        ];
        coroutine_imports
            .into_iter()
            .chain(stream_imports)
            .chain(builtin_imports)
            .for_each(|import| {
                if !imports.iter().any(|item| item == &import) {
                    imports.push(import);
                }
            });
        imports
    }

    fn collect_builtin_ids(&self) -> HashSet<BuiltinId> {
        let mut used = HashSet::new();
        self.contract
            .functions
            .iter()
            .for_each(|function| self.collect_builtins_from_function(function, &mut used));
        self.contract.catalog.all_classes().for_each(|class| {
            class
                .constructors
                .iter()
                .for_each(|ctor| self.collect_builtins_from_constructor(ctor, &mut used));
            class
                .methods
                .iter()
                .for_each(|method| self.collect_builtins_from_method(method, &mut used));
            class
                .streams
                .iter()
                .for_each(|stream| self.collect_builtins_from_type(&stream.item_type, &mut used));
        });
        self.contract.catalog.all_records().for_each(|record| {
            record
                .fields
                .iter()
                .for_each(|field| self.collect_builtins_from_type(&field.type_expr, &mut used))
        });
        self.contract.catalog.all_enums().for_each(|enumeration| {
            if let EnumRepr::Data { variants, .. } = &enumeration.repr {
                variants.iter().for_each(|variant| match &variant.payload {
                    VariantPayload::Struct(fields) => fields.iter().for_each(|field| {
                        self.collect_builtins_from_type(&field.type_expr, &mut used)
                    }),
                    VariantPayload::Tuple(fields) => fields
                        .iter()
                        .for_each(|ty| self.collect_builtins_from_type(ty, &mut used)),
                    VariantPayload::Unit => {}
                })
            }
        });
        self.contract
            .catalog
            .all_custom_types()
            .for_each(|custom| self.collect_builtins_from_type(&custom.repr, &mut used));
        self.contract.catalog.all_callbacks().for_each(|callback| {
            callback.methods.iter().for_each(|method| {
                method
                    .params
                    .iter()
                    .for_each(|param| self.collect_builtins_from_type(&param.type_expr, &mut used));
                self.collect_builtins_from_return(&method.returns, &mut used);
            })
        });
        used
    }

    fn collect_builtins_from_function(&self, func: &FunctionDef, used: &mut HashSet<BuiltinId>) {
        func.params
            .iter()
            .for_each(|param| self.collect_builtins_from_type(&param.type_expr, used));
        self.collect_builtins_from_return(&func.returns, used);
    }

    fn collect_builtins_from_constructor(
        &self,
        ctor: &ConstructorDef,
        used: &mut HashSet<BuiltinId>,
    ) {
        ctor.params()
            .iter()
            .for_each(|param| self.collect_builtins_from_type(&param.type_expr, used));
    }

    fn collect_builtins_from_method(&self, method: &MethodDef, used: &mut HashSet<BuiltinId>) {
        method
            .params
            .iter()
            .for_each(|param| self.collect_builtins_from_type(&param.type_expr, used));
        self.collect_builtins_from_return(&method.returns, used);
    }

    fn collect_builtins_from_return(&self, returns: &ReturnDef, used: &mut HashSet<BuiltinId>) {
        match returns {
            ReturnDef::Void => {}
            ReturnDef::Value(ty) => self.collect_builtins_from_type(ty, used),
            ReturnDef::Result { ok, err } => {
                self.collect_builtins_from_type(ok, used);
                self.collect_builtins_from_type(err, used);
            }
        }
    }

    fn collect_builtins_from_type(&self, ty: &TypeExpr, used: &mut HashSet<BuiltinId>) {
        match ty {
            TypeExpr::Builtin(id) => {
                used.insert(id.clone());
            }
            TypeExpr::Option(inner) | TypeExpr::Vec(inner) => {
                self.collect_builtins_from_type(inner, used)
            }
            TypeExpr::Result { ok, err } => {
                self.collect_builtins_from_type(ok, used);
                self.collect_builtins_from_type(err, used);
            }
            _ => {}
        }
    }

    fn builtin_import(&self, id: &BuiltinId) -> Option<String> {
        match id.as_str() {
            "Duration" => Some("java.time.Duration".to_string()),
            "SystemTime" => Some("java.time.Instant".to_string()),
            "Uuid" => Some("java.util.UUID".to_string()),
            "Url" => Some("java.net.URI".to_string()),
            _ => None,
        }
    }

    fn lower_custom_type(&self, custom: &CustomTypeDef) -> KotlinCustomType {
        let class_name = NamingConvention::class_name(custom.id.as_str());
        let repr_kotlin_type = self.kotlin_type(&custom.repr);
        let custom_seq = self.custom_read_seq(custom);
        let repr_decode_expr = emit::emit_reader_read(&custom_seq);
        let custom_write_seq = self.custom_write_seq(custom);
        let repr_encode_expr = emit::emit_write_expr(&custom_write_seq);
        let repr_size_expr = emit::emit_size_expr_for_write_seq(&custom_write_seq);
        let has_native_mapping = self.type_mappings.contains_key(custom.id.as_str());

        KotlinCustomType {
            class_name,
            repr_kotlin_type,
            repr_size_expr,
            repr_encode_expr,
            repr_decode_expr,
            has_native_mapping,
        }
    }

    fn custom_read_seq(&self, custom: &CustomTypeDef) -> ReadSeq {
        self.find_custom_read_seq(&custom.id)
            .unwrap_or_else(|| self.read_seq_from_repr(&custom.repr))
    }

    fn custom_write_seq(&self, custom: &CustomTypeDef) -> WriteSeq {
        let base_seq = self
            .find_custom_write_seq(&custom.id)
            .unwrap_or_else(|| self.write_seq_from_repr(&custom.repr));
        let remapped = remap_root_in_seq(&base_seq, ValueExpr::Var("repr".to_string()));
        self.normalize_custom_write_seq(&custom.repr, remapped)
    }

    fn normalize_custom_write_seq(&self, repr: &TypeExpr, seq: WriteSeq) -> WriteSeq {
        let _ = repr;
        self.strip_field_access_in_write_seq(&seq)
    }

    fn strip_field_access_in_write_seq(&self, seq: &WriteSeq) -> WriteSeq {
        WriteSeq {
            size: self.strip_field_access_in_size(&seq.size),
            ops: seq
                .ops
                .iter()
                .map(|op| self.strip_field_access_in_write_op(op))
                .collect(),
            shape: seq.shape,
        }
    }

    fn strip_field_access_in_size(&self, size: &SizeExpr) -> SizeExpr {
        match size {
            SizeExpr::Fixed(value) => SizeExpr::Fixed(*value),
            SizeExpr::Runtime => SizeExpr::Runtime,
            SizeExpr::StringLen(value) => {
                SizeExpr::StringLen(self.strip_field_access_in_value(value))
            }
            SizeExpr::BytesLen(value) => {
                SizeExpr::BytesLen(self.strip_field_access_in_value(value))
            }
            SizeExpr::ValueSize(value) => {
                SizeExpr::ValueSize(self.strip_field_access_in_value(value))
            }
            SizeExpr::WireSize { value, record_id } => SizeExpr::WireSize {
                value: self.strip_field_access_in_value(value),
                record_id: record_id.clone(),
            },
            SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
                id: id.clone(),
                value: self.strip_field_access_in_value(value),
            },
            SizeExpr::Sum(parts) => SizeExpr::Sum(
                parts
                    .iter()
                    .map(|part| self.strip_field_access_in_size(part))
                    .collect(),
            ),
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: self.strip_field_access_in_value(value),
                inner: Box::new(self.strip_field_access_in_size(inner)),
            },
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: self.strip_field_access_in_value(value),
                inner: Box::new(self.strip_field_access_in_size(inner)),
                layout: layout.clone(),
            },
            SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
                value: self.strip_field_access_in_value(value),
                ok: Box::new(self.strip_field_access_in_size(ok)),
                err: Box::new(self.strip_field_access_in_size(err)),
            },
        }
    }

    fn strip_field_access_in_write_op(&self, op: &WriteOp) -> WriteOp {
        match op {
            WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
                primitive: *primitive,
                value: self.strip_field_access_in_value(value),
            },
            WriteOp::String { value } => WriteOp::String {
                value: self.strip_field_access_in_value(value),
            },
            WriteOp::Bytes { value } => WriteOp::Bytes {
                value: self.strip_field_access_in_value(value),
            },
            WriteOp::Builtin { id, value } => WriteOp::Builtin {
                id: id.clone(),
                value: self.strip_field_access_in_value(value),
            },
            WriteOp::Option { value, some } => WriteOp::Option {
                value: self.strip_field_access_in_value(value),
                some: Box::new(self.strip_field_access_in_write_seq(some)),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: self.strip_field_access_in_value(value),
                element_type: element_type.clone(),
                element: Box::new(self.strip_field_access_in_write_seq(element)),
                layout: layout.clone(),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: self.strip_field_access_in_value(value),
                fields: fields
                    .iter()
                    .map(|field| FieldWriteOp {
                        name: field.name.clone(),
                        accessor: self.strip_field_access_in_value(&field.accessor),
                        seq: self.strip_field_access_in_write_seq(&field.seq),
                    })
                    .collect(),
            },
            WriteOp::Enum { id, value, layout } => WriteOp::Enum {
                id: id.clone(),
                value: self.strip_field_access_in_value(value),
                layout: layout.clone(),
            },
            WriteOp::Result { value, ok, err } => WriteOp::Result {
                value: self.strip_field_access_in_value(value),
                ok: Box::new(self.strip_field_access_in_write_seq(ok)),
                err: Box::new(self.strip_field_access_in_write_seq(err)),
            },
            WriteOp::Custom {
                id,
                value,
                underlying,
            } => WriteOp::Custom {
                id: id.clone(),
                value: self.strip_field_access_in_value(value),
                underlying: Box::new(self.strip_field_access_in_write_seq(underlying)),
            },
        }
    }

    fn strip_field_access_in_value(&self, value: &ValueExpr) -> ValueExpr {
        match value {
            ValueExpr::Field(parent, name) => {
                let stripped_parent = self.strip_field_access_in_value(parent);
                match &stripped_parent {
                    ValueExpr::Var(var) if var == "repr" => ValueExpr::Var("repr".to_string()),
                    ValueExpr::Named(name) if name == "repr" => ValueExpr::Var("repr".to_string()),
                    _ => ValueExpr::Field(Box::new(stripped_parent), name.clone()),
                }
            }
            ValueExpr::Instance => ValueExpr::Var("repr".to_string()),
            ValueExpr::Var(_) | ValueExpr::Named(_) => value.clone(),
        }
    }

    fn lower_enum(&self, enumeration: &EnumDef) -> KotlinEnum {
        let abi_enum = self.abi_enum_for(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let kind = if enumeration.is_error {
            KotlinEnumKind::Error
        } else if abi_enum.is_c_style {
            KotlinEnumKind::CStyle
        } else {
            KotlinEnumKind::Sealed
        };
        let variant_names = abi_enum
            .variants
            .iter()
            .map(|variant| NamingConvention::class_name(variant.name.as_str()))
            .collect::<HashSet<_>>();
        let variant_docs = enumeration.variant_docs();
        let variants = abi_enum
            .variants
            .iter()
            .enumerate()
            .map(|(i, variant)| {
                let mut v = self.lower_enum_variant(variant, kind, &variant_names);
                v.doc = variant_docs.get(i).cloned().flatten();
                v
            })
            .collect::<Vec<_>>();
        KotlinEnum {
            class_name,
            variants,
            kind,
            doc: enumeration.doc.clone(),
        }
    }

    fn lower_enum_variant(
        &self,
        variant: &AbiEnumVariant,
        kind: KotlinEnumKind,
        variant_names: &HashSet<String>,
    ) -> KotlinEnumVariant {
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|field| self.lower_enum_field(field, variant_names))
                .collect(),
        };
        let name = match kind {
            KotlinEnumKind::CStyle => NamingConvention::enum_entry_name(variant.name.as_str()),
            _ => NamingConvention::class_name(variant.name.as_str()),
        };
        KotlinEnumVariant {
            name,
            tag: variant.discriminant,
            fields,
            doc: None,
        }
    }

    fn lower_enum_field(
        &self,
        field: &AbiEnumField,
        variant_names: &HashSet<String>,
    ) -> KotlinEnumField {
        let (kotlin_type, decode_name) =
            self.kotlin_type_with_disambiguation(&field.type_expr, variant_names);
        let wire_decode_expr = emit::emit_reader_read(&field.decode);
        let wire_decode_expr = self.qualify_decode_expr(wire_decode_expr, decode_name.as_deref());
        KotlinEnumField {
            name: NamingConvention::property_name(field.name.as_str()),
            kotlin_type,
            wire_decode_expr,
            wire_size_expr: emit::emit_size_expr_for_write_seq(&field.encode),
            wire_encode: emit::emit_write_expr(&field.encode),
        }
    }

    fn lower_data_enum_codec(&self, enumeration: &EnumDef) -> KotlinDataEnumCodec {
        let layout = self.data_enum_layout(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let codec_name = format!("{}Codec", class_name);
        let variants = match (&enumeration.repr, &layout) {
            (EnumRepr::Data { variants, .. }, Some(layout)) => variants
                .iter()
                .enumerate()
                .map(|(index, variant)| KotlinDataEnumVariant {
                    name: NamingConvention::class_name(variant.name.as_str()),
                    const_name: variant.name.as_str().to_uppercase(),
                    tag_value: variant.discriminant,
                    fields: self.lower_data_enum_codec_fields(
                        &variant.payload,
                        layout.variant_offsets.get(index),
                    ),
                })
                .collect(),
            _ => Vec::new(),
        };
        KotlinDataEnumCodec {
            class_name,
            codec_name,
            struct_size: layout.as_ref().map(|l| l.struct_size).unwrap_or(0),
            payload_offset: layout.as_ref().map(|l| l.payload_offset).unwrap_or(0),
            variants,
        }
    }

    fn lower_data_enum_codec_fields(
        &self,
        payload: &VariantPayload,
        offsets: Option<&Vec<usize>>,
    ) -> Vec<KotlinDataEnumField> {
        let Some(offsets) = offsets else {
            return Vec::new();
        };

        match payload {
            VariantPayload::Unit => Vec::new(),
            VariantPayload::Struct(fields) => fields
                .iter()
                .zip(offsets.iter().copied())
                .filter_map(|(field, offset)| match &field.type_expr {
                    TypeExpr::Primitive(primitive) => {
                        let param_name = NamingConvention::property_name(field.name.as_str());
                        Some(self.data_enum_field_for_primitive(*primitive, param_name, offset))
                    }
                    _ => None,
                })
                .collect(),
            VariantPayload::Tuple(types) => types
                .iter()
                .enumerate()
                .zip(offsets.iter().copied())
                .filter_map(|((index, type_expr), offset)| match type_expr {
                    TypeExpr::Primitive(primitive) => {
                        let base_name = format!("value_{}", index);
                        let param_name = NamingConvention::property_name(base_name.as_str());
                        Some(self.data_enum_field_for_primitive(*primitive, param_name, offset))
                    }
                    _ => None,
                })
                .collect(),
        }
    }

    fn data_enum_field_for_primitive(
        &self,
        primitive: PrimitiveType,
        param_name: String,
        offset: usize,
    ) -> KotlinDataEnumField {
        let (getter, putter, conversion) = self.primitive_field_accessors(primitive);
        let value_expr =
            self.primitive_write_value_expr(primitive, &format!("value.{}", param_name));
        KotlinDataEnumField {
            param_name,
            value_expr,
            offset,
            getter,
            putter,
            conversion,
        }
    }

    fn lower_record(&self, record: &RecordDef) -> KotlinRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(record, field))
            .collect::<Vec<_>>();
        KotlinRecord {
            class_name,
            fields,
            is_blittable: record.is_blittable(),
            struct_size: self.record_struct_size(record.id.as_str()),
            doc: record.doc.clone(),
        }
    }

    fn lower_record_field(&self, record: &RecordDef, field: &FieldDef) -> KotlinRecordField {
        let decode_seq = self
            .record_field_read_seq(&record.id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(&record.id, &field.name)
            .expect("record field encode ops");
        KotlinRecordField {
            name: NamingConvention::property_name(field.name.as_str()),
            kotlin_type: self.kotlin_type(&field.type_expr),
            default_value: field
                .default
                .as_ref()
                .map(|d| kotlin_default_literal(d, &self.kotlin_type(&field.type_expr))),
            wire_decode_expr: emit::emit_reader_read(&decode_seq),
            wire_size_expr: emit::emit_size_expr_for_write_seq(&encode_seq),
            wire_encode: emit::emit_write_expr(&encode_seq),
            padding_after: self.field_padding_after(&record.id, &field.name),
            doc: field.doc.clone(),
        }
    }

    fn lower_record_readers(&self) -> Vec<KotlinRecordReader> {
        let record_ids = self.blittable_return_record_ids();
        self.contract
            .catalog
            .all_records()
            .filter(|record| record_ids.contains(record.id.as_str()))
            .filter_map(|record| {
                let fields = self.record_blittable_fields(&record.id)?;
                let reader_name =
                    format!("{}Reader", NamingConvention::class_name(record.id.as_str()));
                Some(KotlinRecordReader {
                    reader_name,
                    class_name: NamingConvention::class_name(record.id.as_str()),
                    struct_size: self.record_struct_size(record.id.as_str()),
                    fields: fields
                        .iter()
                        .map(|field| {
                            let (getter, _, conversion) =
                                self.primitive_field_accessors(field.primitive);
                            KotlinRecordReaderField {
                                name: NamingConvention::property_name(field.name.as_str()),
                                const_name: field.name.as_str().to_uppercase(),
                                offset: field.offset,
                                getter,
                                conversion,
                            }
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn lower_record_writers(&self) -> Vec<KotlinRecordWriter> {
        let record_ids = self.blittable_vec_param_records();
        self.contract
            .catalog
            .all_records()
            .filter(|record| record_ids.contains(record.id.as_str()))
            .filter_map(|record| {
                let fields = self.record_blittable_fields(&record.id)?;
                let writer_name =
                    format!("{}Writer", NamingConvention::class_name(record.id.as_str()));
                Some(KotlinRecordWriter {
                    writer_name,
                    class_name: NamingConvention::class_name(record.id.as_str()),
                    struct_size: self.record_struct_size(record.id.as_str()),
                    fields: fields
                        .iter()
                        .map(|field| {
                            let (_, putter, _) = self.primitive_field_accessors(field.primitive);
                            let value_expr = self.primitive_write_value_expr(
                                field.primitive,
                                &format!(
                                    "item.{}",
                                    NamingConvention::property_name(field.name.as_str())
                                ),
                            );
                            KotlinRecordWriterField {
                                const_name: field.name.as_str().to_uppercase(),
                                offset: field.offset,
                                putter,
                                value_expr,
                            }
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn lower_closures(&self) -> Vec<KotlinClosureInterface> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|callback| matches!(callback.kind, CallbackKind::Closure))
            .filter_map(|callback| callback.methods.first().map(|method| (callback, method)))
            .map(|(callback, method)| KotlinClosureInterface {
                interface_name: self.closure_interface_name(callback.id.as_str()),
                params: method
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| KotlinSignatureParam {
                        name: format!("p{}", index),
                        kotlin_type: self.closure_param_type(&param.type_expr),
                    })
                    .collect(),
                return_type: match &method.returns {
                    ReturnDef::Void => None,
                    _ => Some(self.kotlin_type_from_return_def(&method.returns)),
                },
            })
            .collect()
    }

    fn lower_function(&self, func: &FunctionDef) -> KotlinFunction {
        let call = self.abi_call_for_function(func);
        let output_route = call_output_binding(call);
        let signature_params = func
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .map(|w| w.binding_name.clone())
            .collect();
        let native_args = self.native_args_for_params(call, &wire_writers);
        let return_type = self.kotlin_return_type_from_def(&func.returns, &output_route);
        let return_meta = self.kotlin_return_meta(&output_route);
        let decode_expr = self.decode_expr_for_call_return(&output_route, &func.returns);
        let is_blittable_return = self.is_blittable_return(&output_route);
        let async_call = match &call.mode {
            CallMode::Async(_) => Some(self.async_call_for_function(func, call)),
            CallMode::Sync => None,
        };
        KotlinFunction {
            func_name: NamingConvention::method_name(func.id.as_str()),
            signature_params,
            return_type,
            wire_writers,
            wire_writer_closes,
            native_args,
            throws: self.is_throwing_return(&func.returns),
            err_type: self.error_type_name(&func.returns),
            ffi_name: call.symbol.as_str().to_string(),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            return_cast: return_meta.cast,
            async_call,
            decode_expr,
            is_blittable_return,
            doc: func.doc.clone(),
        }
    }

    fn lower_class(&self, class: &ClassDef) -> KotlinClass {
        let class_name = NamingConvention::class_name(class.id.as_str());
        let constructors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, ctor)| self.lower_constructor(class, ctor, index))
            .collect::<Vec<_>>();
        let methods = class
            .methods
            .iter()
            .map(|method| self.lower_method(class, method))
            .collect::<Vec<_>>();
        let streams = class
            .streams
            .iter()
            .map(|stream| {
                let abi_stream = self.abi_stream(class, stream);
                self.lower_stream(stream, abi_stream, &class_name)
            })
            .collect::<Vec<_>>();
        KotlinClass {
            class_name,
            doc: class.doc.clone(),
            prefix: naming::ffi_prefix().to_string(),
            ffi_free: naming::class_ffi_free(class.id.as_str()).into_string(),
            constructors,
            methods,
            streams,
            use_companion_methods: matches!(
                self.options.factory_style,
                FactoryStyle::CompanionMethods
            ),
        }
    }

    fn lower_constructor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        index: usize,
    ) -> KotlinConstructor {
        let call = self.abi_call_for_constructor(class, index);
        let (name, is_factory) = match ctor {
            ConstructorDef::Default { .. } => ("new".to_string(), false),
            ConstructorDef::NamedFactory { name, .. } => (name.as_str().to_string(), true),
            ConstructorDef::NamedInit { name, .. } => (name.as_str().to_string(), true),
        };
        let signature_params = ctor
            .params()
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .map(|w| w.binding_name.clone())
            .collect();
        let native_args = self.native_args_for_params(call, &wire_writers);
        KotlinConstructor {
            name: NamingConvention::method_name(&name),
            is_factory,
            is_fallible: ctor.is_fallible(),
            signature_params,
            wire_writers,
            wire_writer_closes,
            native_args,
            ffi_name: call.symbol.as_str().to_string(),
            doc: ctor.doc().map(String::from),
        }
    }

    fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> KotlinMethod {
        let call = Self::strip_receiver(self.abi_call_for_method(class, method));
        let call = &call;
        let output_route = call_output_binding(call);
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .map(|w| w.binding_name.clone())
            .collect();
        let native_args = self.native_args_for_params(call, &wire_writers);
        let signature_params = method
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let return_type = self.kotlin_return_type_from_def(&method.returns, &output_route);
        let return_meta = self.kotlin_return_meta(&output_route);
        let decode_expr = self.decode_expr_for_call_return(&output_route, &method.returns);
        let is_blittable_return = self.is_blittable_return(&output_route);
        let ffi_name = call.symbol.as_str().to_string();
        let include_handle = method.receiver != Receiver::Static;
        let err_type = self.error_type_name(&method.returns);
        let rendered = if method.is_async {
            let async_call = self.async_call_for_method(class, method, call);
            AsyncMethodTemplate {
                method_name: &NamingConvention::method_name(method.id.as_str()),
                signature_params: &signature_params,
                return_type: return_type.as_deref(),
                wire_writers: &wire_writers,
                wire_writer_closes: &wire_writer_closes,
                native_args: &native_args,
                throws: self.is_throwing_return(&method.returns),
                err_type: &err_type,
                ffi_name: &ffi_name,
                include_handle,
                ffi_poll: &async_call.poll,
                ffi_complete: &async_call.complete,
                ffi_cancel: &async_call.cancel,
                ffi_free: &async_call.free,
                return_is_unit: async_call.return_is_unit,
                return_is_direct: async_call.return_is_direct,
                return_cast: &async_call.return_cast,
                decode_expr: &async_call.decode_expr,
                is_blittable_return: async_call.is_blittable_return,
                doc: &method.doc,
            }
            .render()
            .unwrap()
        } else {
            WireMethodTemplate {
                method_name: &NamingConvention::method_name(method.id.as_str()),
                signature_params: &signature_params,
                return_type: return_type.as_deref(),
                wire_writers: &wire_writers,
                wire_writer_closes: &wire_writer_closes,
                native_args: &native_args,
                throws: self.is_throwing_return(&method.returns),
                err_type: &err_type,
                ffi_name: &ffi_name,
                return_is_unit: return_meta.is_unit,
                return_is_direct: return_meta.is_direct,
                return_cast: &return_meta.cast,
                decode_expr: &decode_expr,
                is_blittable_return,
                include_handle,
                doc: &method.doc,
            }
            .render()
            .unwrap()
        };
        KotlinMethod {
            impl_: if method.is_async {
                KotlinMethodImpl::AsyncMethod(rendered)
            } else {
                KotlinMethodImpl::SyncMethod(rendered)
            },
            is_static: method.receiver == Receiver::Static,
        }
    }

    fn lower_stream(
        &self,
        stream_def: &StreamDef,
        stream: &AbiStream,
        class_name: &str,
    ) -> KotlinStream {
        let StreamItemTransport::WireEncoded { decode_ops } = &stream.item;
        let method_name_pascal = NamingConvention::class_name(stream.stream_id.as_str());
        let mode = match stream.mode {
            StreamMode::Async => KotlinStreamMode::Async,
            StreamMode::Batch => KotlinStreamMode::Batch {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
            StreamMode::Callback => KotlinStreamMode::Callback {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
        };
        KotlinStream {
            name: NamingConvention::method_name(stream.stream_id.as_str()),
            mode,
            item_type: self.kotlin_type(&stream_def.item_type),
            item_decode: self.rebase_read_seq(decode_ops, "pos", "0"),
            subscribe: stream.subscribe.to_string(),
            poll: stream.poll.to_string(),
            pop_batch: stream.pop_batch.to_string(),
            wait: stream.wait.to_string(),
            unsubscribe: stream.unsubscribe.to_string(),
            free: stream.free.to_string(),
        }
    }

    fn abi_stream<'b>(&'b self, class: &ClassDef, stream: &StreamDef) -> &'b AbiStream {
        self.abi
            .streams
            .iter()
            .find(|item| item.class_id == class.id && item.stream_id == stream.id)
            .expect("abi stream")
    }

    fn rebase_read_seq(&self, seq: &ReadSeq, old_base: &str, new_base: &str) -> ReadSeq {
        ReadSeq {
            size: seq.size.clone(),
            ops: seq
                .ops
                .iter()
                .map(|op| self.rebase_read_op(op, old_base, new_base))
                .collect(),
            shape: seq.shape,
        }
    }

    fn rebase_read_op(&self, op: &ReadOp, old_base: &str, new_base: &str) -> ReadOp {
        match op {
            ReadOp::Primitive { primitive, offset } => ReadOp::Primitive {
                primitive: *primitive,
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::String { offset } => ReadOp::String {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Bytes { offset } => ReadOp::Bytes {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Option { tag_offset, some } => ReadOp::Option {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                some: Box::new(self.rebase_read_seq(some, old_base, new_base)),
            },
            ReadOp::Vec {
                len_offset,
                element_type,
                element,
                layout,
            } => ReadOp::Vec {
                len_offset: self.rebase_offset_expr(len_offset, old_base, new_base),
                element_type: element_type.clone(),
                element: Box::new(self.rebase_read_seq(element, old_base, new_base)),
                layout: layout.clone(),
            },
            ReadOp::Record { id, offset, fields } => ReadOp::Record {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                fields: fields
                    .iter()
                    .map(|field| {
                        let seq = self.rebase_read_seq(&field.seq, old_base, new_base);
                        FieldReadOp {
                            name: field.name.clone(),
                            seq,
                        }
                    })
                    .collect(),
            },
            ReadOp::Enum { id, offset, layout } => ReadOp::Enum {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                layout: layout.clone(),
            },
            ReadOp::Result {
                tag_offset,
                ok,
                err,
            } => ReadOp::Result {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                ok: Box::new(self.rebase_read_seq(ok, old_base, new_base)),
                err: Box::new(self.rebase_read_seq(err, old_base, new_base)),
            },
            ReadOp::Builtin { id, offset } => ReadOp::Builtin {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Custom { id, underlying } => ReadOp::Custom {
                id: id.clone(),
                underlying: Box::new(self.rebase_read_seq(underlying, old_base, new_base)),
            },
        }
    }

    fn rebase_offset_expr(
        &self,
        offset: &OffsetExpr,
        old_base: &str,
        new_base: &str,
    ) -> OffsetExpr {
        match offset {
            OffsetExpr::Fixed(value) => OffsetExpr::Fixed(*value),
            OffsetExpr::Base => OffsetExpr::Base,
            OffsetExpr::BasePlus(add) => OffsetExpr::BasePlus(*add),
            OffsetExpr::Var(name) => {
                if name == old_base {
                    OffsetExpr::Var(new_base.to_string())
                } else {
                    OffsetExpr::Var(name.clone())
                }
            }
            OffsetExpr::VarPlus(name, add) => {
                if name == old_base {
                    OffsetExpr::VarPlus(new_base.to_string(), *add)
                } else {
                    OffsetExpr::VarPlus(name.clone(), *add)
                }
            }
        }
    }

    fn lower_callback_trait(&self, callback: &CallbackTraitDef) -> KotlinCallbackTrait {
        let interface_name = NamingConvention::class_name(callback.id.as_str());
        let handle_map_name = format!("{}HandleMap", interface_name);
        let callbacks_object = format!("{}Callbacks", interface_name);
        let bridge_name = format!("{}Bridge", interface_name);
        let sync_methods = callback
            .methods
            .iter()
            .filter(|method| !method.is_async)
            .map(|method| self.lower_callback_method(callback, method))
            .collect();
        let async_methods = callback
            .methods
            .iter()
            .filter(|method| method.is_async)
            .map(|method| self.lower_async_callback_method(callback, method))
            .collect();
        KotlinCallbackTrait {
            interface_name,
            handle_map_name,
            callbacks_object,
            bridge_name,
            doc: callback.doc.clone(),
            is_closure: matches!(callback.kind, CallbackKind::Closure),
            sync_methods,
            async_methods,
        }
    }

    fn lower_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinCallbackMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let output_route = callback_output_binding(abi_method);
        let abi_param_map: HashMap<_, _> = abi_method
            .params
            .iter()
            .map(|param| (param.name.clone(), param))
            .collect();
        let params = method
            .params
            .iter()
            .filter_map(|def| {
                let abi_param = abi_param_map.get(&def.name)?;
                Some(self.lower_callback_param(def, abi_param))
            })
            .collect();
        let return_info =
            self.callback_return_info(&method.returns, &output_route, &abi_method.error);
        KotlinCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            params,
            return_info,
            doc: method.doc.clone(),
        }
    }

    fn lower_async_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinAsyncCallbackMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let output_route = callback_output_binding(abi_method);
        let abi_param_map: HashMap<_, _> = abi_method
            .params
            .iter()
            .map(|param| (param.name.clone(), param))
            .collect();
        let params = method
            .params
            .iter()
            .filter_map(|def| {
                let abi_param = abi_param_map.get(&def.name)?;
                Some(self.lower_callback_param(def, abi_param))
            })
            .collect();
        let return_info =
            self.callback_return_info(&method.returns, &output_route, &abi_method.error);
        let invoker = self.async_callback_invoker(&return_info);
        KotlinAsyncCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            invoker_name: invoker.name,
            params,
            return_info,
            doc: method.doc.clone(),
        }
    }

    fn lower_callback_param(&self, def: &ParamDef, param: &AbiParam) -> KotlinCallbackParam {
        let name = NamingConvention::param_name(param.name.as_str());
        let kotlin_type = self.kotlin_type(&def.type_expr);
        match param.input_binding().expect("callback param role") {
            InputBinding::Scalar => KotlinCallbackParam {
                name: name.clone(),
                kotlin_type,
                jni_type: self.jni_type_for_abi(&param.ffi_type),
                conversion: self.callback_direct_conversion(&def.type_expr, &name),
            },
            InputBinding::WirePacket { decode_ops, .. } => KotlinCallbackParam {
                name: name.clone(),
                kotlin_type,
                jni_type: "ByteBuffer".to_string(),
                conversion: self.callback_encoded_conversion(decode_ops, &name),
            },
            _ => unreachable!(
                "unsupported callback param role: {:?}",
                param_binding(param)
            ),
        }
    }

    fn async_callback_invoker(
        &self,
        return_info: &Option<KotlinCallbackReturn>,
    ) -> KotlinAsyncCallbackInvoker {
        let mut result_jni_type = return_info.as_ref().map(|ret| ret.jni_type.clone());
        let suffix = self.invoker_suffix_from_jni_type(&result_jni_type);
        if suffix == "Void" {
            result_jni_type = None;
        }
        KotlinAsyncCallbackInvoker {
            name: format!("invokeAsyncCallback{}", suffix),
            result_jni_type,
        }
    }

    fn invoker_suffix_from_jni_type(&self, result_jni_type: &Option<String>) -> String {
        match result_jni_type.as_deref() {
            None => "Void".to_string(),
            Some("Boolean") => "Bool".to_string(),
            Some("Byte") => "I8".to_string(),
            Some("Short") => "I16".to_string(),
            Some("Int") => "I32".to_string(),
            Some("Long") => "I64".to_string(),
            Some("Float") => "F32".to_string(),
            Some("Double") => "F64".to_string(),
            Some("ByteArray") => "Wire".to_string(),
            Some("ByteBuffer") => "Object".to_string(),
            Some("String") => "Object".to_string(),
            Some(_) => "Object".to_string(),
        }
    }

    fn callback_return_info(
        &self,
        returns: &ReturnDef,
        transport: &OutputBinding,
        error: &ErrorTransport,
    ) -> Option<KotlinCallbackReturn> {
        let kotlin_type = self.kotlin_return_type_from_def(returns, transport)?;
        let (jni_type, default_value, to_jni) = match transport {
            OutputBinding::Unit => return None,
            OutputBinding::Fast(FastOutputBinding::Scalar { abi_type }) => (
                self.jni_type_for_abi(abi_type),
                self.callback_default_value_for_abi(abi_type),
                self.callback_return_cast_for_abi(abi_type),
            ),
            OutputBinding::Fast(_) | OutputBinding::Wire(_) => (
                "ByteArray".to_string(),
                "byteArrayOf()".to_string(),
                self.callback_return_wire_encode(transport.encode_ops().expect("encoded return")),
            ),
            OutputBinding::Handle { class_id, nullable } => (
                "Long".to_string(),
                "0L".to_string(),
                self.callback_return_handle_cast(class_id, *nullable),
            ),
            OutputBinding::CallbackHandle {
                callback_id,
                nullable,
            } => (
                "Long".to_string(),
                "0L".to_string(),
                self.callback_return_callback_cast(callback_id, *nullable),
            ),
        };
        let (error_type, error_is_throwable) = match returns {
            ReturnDef::Result { err, .. } => {
                let err_type = self.kotlin_type(err);
                let throwable = match err {
                    TypeExpr::Enum(id) => self
                        .contract
                        .catalog
                        .resolve_enum(id)
                        .map(|e| e.is_error)
                        .unwrap_or(false),
                    _ => false,
                };
                (Some(err_type), throwable)
            }
            _ => (None, false),
        };
        let to_jni_result = self.build_result_wire_encode(transport, error);
        Some(KotlinCallbackReturn {
            kotlin_type,
            jni_type,
            default_value,
            to_jni,
            to_jni_result,
            error_type,
            error_is_throwable,
        })
    }

    fn callback_direct_conversion(&self, ty: &TypeExpr, name: &str) -> String {
        match ty {
            TypeExpr::Primitive(p) => match p {
                PrimitiveType::U8 => format!("{}.toUByte()", name),
                PrimitiveType::U16 => format!("{}.toUShort()", name),
                PrimitiveType::U32 => format!("{}.toUInt()", name),
                PrimitiveType::U64 | PrimitiveType::USize => format!("{}.toULong()", name),
                _ => name.to_string(),
            },
            _ => name.to_string(),
        }
    }

    fn callback_encoded_conversion(&self, decode_ops: &ReadSeq, name: &str) -> String {
        let decode_expr = emit::emit_reader_read(decode_ops);
        format!(
            "run {{ val bytes = ByteArray({}.remaining()); {}.get(bytes); val reader = WireReader(bytes); {} }}",
            name, name, decode_expr
        )
    }

    fn callback_return_cast_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::U8 => ".toByte()".to_string(),
            AbiType::U16 => ".toShort()".to_string(),
            AbiType::U32 => ".toInt()".to_string(),
            AbiType::U64 | AbiType::USize => ".toLong()".to_string(),
            _ => String::new(),
        }
    }

    fn callback_default_value_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "false".to_string(),
            AbiType::I8 | AbiType::U8 => "0".to_string(),
            AbiType::I16 | AbiType::U16 => "0".to_string(),
            AbiType::I32 | AbiType::U32 => "0".to_string(),
            AbiType::I64 | AbiType::U64 | AbiType::ISize | AbiType::USize => "0L".to_string(),
            AbiType::F32 => "0f".to_string(),
            AbiType::F64 => "0.0".to_string(),
            _ => "0".to_string(),
        }
    }

    fn callback_return_handle_cast(&self, _class_id: &ClassId, nullable: bool) -> String {
        if nullable {
            "?.handle ?: 0L".to_string()
        } else {
            ".handle".to_string()
        }
    }

    fn callback_return_callback_cast(&self, callback_id: &CallbackId, nullable: bool) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!("?.let {{ {}.create(it) }} ?: 0L", bridge)
        } else {
            format!(".let {{ {}.create(it) }}", bridge)
        }
    }

    fn callback_return_wire_encode(&self, encode_ops: &WriteSeq) -> String {
        let size_expr = emit::emit_size_expr_for_write_seq(encode_ops);
        let encode_expr = emit::emit_write_expr(encode_ops);
        format!(
            ".let {{ value -> run {{ val writer = WireWriterPool.acquire({}); try {{ val wire = writer.writer; {}; wire.toByteArray() }} finally {{ writer.close() }} }} }}",
            size_expr, encode_expr
        )
    }

    fn build_result_wire_encode(
        &self,
        transport: &OutputBinding,
        error: &ErrorTransport,
    ) -> Option<String> {
        let ok_encode_ops = transport.encode_ops()?;
        let err_encode_ops = match error {
            ErrorTransport::Encoded {
                encode_ops: Some(err_encode_ops),
                ..
            } => err_encode_ops,
            _ => return None,
        };

        let ok_seq = remap_root_in_seq(ok_encode_ops, ValueExpr::Var("okVal".to_string()));
        let err_seq = remap_root_in_seq(err_encode_ops, ValueExpr::Var("errVal".to_string()));
        let ok_size = Self::size_expr_for_write_seq(&ok_seq);
        let err_size = Self::size_expr_for_write_seq(&err_seq);
        let value = ValueExpr::Var("value".to_string());

        let result_seq = WriteSeq {
            size: SizeExpr::ResultSize {
                value: value.clone(),
                ok: Box::new(ok_size),
                err: Box::new(err_size),
            },
            ops: vec![WriteOp::Result {
                value: value.clone(),
                ok: Box::new(ok_seq),
                err: Box::new(err_seq),
            }],
            shape: WireShape::Value,
        };

        Some(self.callback_return_wire_encode(&result_seq))
    }

    fn size_expr_for_write_seq(seq: &WriteSeq) -> SizeExpr {
        match seq.ops.first() {
            Some(WriteOp::Custom { value, .. }) => SizeExpr::WireSize {
                value: value.clone(),
                record_id: None,
            },
            _ => seq.size.clone(),
        }
    }

    fn lower_native(&self) -> KotlinNative {
        let functions = self
            .contract
            .functions
            .iter()
            .map(|func| self.lower_native_function(func))
            .collect::<Vec<_>>();
        let class_symbols =
            self.contract
                .catalog
                .all_classes()
                .flat_map(|class| {
                    let ctor_symbols = class.constructors.iter().enumerate().map(|(index, _)| {
                        self.abi_call_for_constructor(class, index).symbol.clone()
                    });
                    let method_symbols = class
                        .methods
                        .iter()
                        .map(|method| self.abi_call_for_method(class, method).symbol.clone());
                    ctor_symbols.chain(method_symbols)
                })
                .collect::<HashSet<_>>();
        let declared_symbols = functions
            .iter()
            .map(|f| f.ffi_name.as_str())
            .chain(class_symbols.iter().map(|s| s.as_str()))
            .collect::<HashSet<_>>();
        let wire_functions = self
            .abi
            .calls
            .iter()
            .filter(|call| call_output_binding(call).decode_ops().is_some())
            .filter(|call| !declared_symbols.contains(call.symbol.as_str()))
            .map(|call| KotlinNativeWireFunction {
                ffi_name: call.symbol.as_str().to_string(),
                params: self
                    .visible_native_params(call)
                    .into_iter()
                    .map(|param| KotlinNativeParam {
                        name: param.name.as_str().to_string(),
                        jni_type: self.jni_type_for_param(param),
                    })
                    .collect(),
                return_jni_type: "ByteArray?".to_string(),
            })
            .collect::<Vec<_>>();
        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_native_class(class))
            .collect::<Vec<_>>();
        let async_callback_invokers = self
            .contract
            .catalog
            .all_callbacks()
            .flat_map(|callback| {
                callback
                    .methods
                    .iter()
                    .filter(|method| method.is_async)
                    .map(|method| {
                        let abi_method = self.abi_callback_method(&callback.id, &method.id);
                        let return_info = self.callback_return_info(
                            &method.returns,
                            &callback_output_binding(abi_method),
                            &abi_method.error,
                        );
                        self.async_callback_invoker(&return_info)
                    })
            })
            .fold(
                (HashSet::new(), Vec::new()),
                |(mut seen, mut invokers), invoker| {
                    if seen.insert(invoker.name.clone()) {
                        invokers.push(invoker);
                    }
                    (seen, invokers)
                },
            )
            .1;
        KotlinNative {
            lib_name: self
                .options
                .library_name
                .clone()
                .unwrap_or_else(|| self.contract.package.name.clone()),
            prefix: naming::ffi_prefix().to_string(),
            functions,
            wire_functions,
            classes,
            async_callback_invokers,
        }
    }

    fn lower_native_function(&self, func: &FunctionDef) -> KotlinNativeFunction {
        let call = self.abi_call_for_function(func);
        let return_jni_type = self.jni_type_for_output(call.output_binding());
        let complete_return_jni_type = match &call.mode {
            CallMode::Async(async_call) => self.jni_type_for_output(async_call.result_binding()),
            CallMode::Sync => String::new(),
        };
        let async_ffi = match &call.mode {
            CallMode::Async(async_call) => Some(KotlinNativeAsyncFfi {
                ffi_poll: async_call.poll.as_str().to_string(),
                ffi_complete: async_call.complete.as_str().to_string(),
                ffi_cancel: async_call.cancel.as_str().to_string(),
                ffi_free: async_call.free.as_str().to_string(),
                complete_return_jni_type,
            }),
            CallMode::Sync => None,
        };
        KotlinNativeFunction {
            ffi_name: call.symbol.as_str().to_string(),
            params: self
                .visible_native_params(call)
                .into_iter()
                .map(|param| KotlinNativeParam {
                    name: param.name.as_str().to_string(),
                    jni_type: self.jni_type_for_param(param),
                })
                .collect(),
            return_jni_type,
            async_ffi,
        }
    }

    fn lower_native_class(&self, class: &ClassDef) -> KotlinNativeClass {
        let ctors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, _ctor)| {
                let call = self.abi_call_for_constructor(class, index);
                KotlinNativeCtor {
                    ffi_name: call.symbol.as_str().to_string(),
                    params: self
                        .visible_native_params(call)
                        .into_iter()
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect(),
                }
            })
            .collect();
        let async_methods = class
            .methods
            .iter()
            .filter(|m| m.is_async)
            .map(|method| {
                let call = Self::strip_receiver(self.abi_call_for_method(class, method));
                let async_call = match &call.mode {
                    CallMode::Async(async_call) => async_call,
                    CallMode::Sync => unreachable!("async method missing async call"),
                };
                KotlinNativeAsyncMethod {
                    ffi_name: call.symbol.as_str().to_string(),
                    ffi_poll: async_call.poll.as_str().to_string(),
                    ffi_complete: async_call.complete.as_str().to_string(),
                    ffi_cancel: async_call.cancel.as_str().to_string(),
                    ffi_free: async_call.free.as_str().to_string(),
                    include_handle: method.receiver != Receiver::Static,
                    params: self
                        .visible_native_params(&call)
                        .into_iter()
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect(),
                    return_jni_type: self.jni_type_for_output(async_call.result_binding()),
                }
            })
            .collect();
        let sync_methods = class
            .methods
            .iter()
            .filter(|m| !m.is_async)
            .map(|method| {
                let call = Self::strip_receiver(self.abi_call_for_method(class, method));
                KotlinNativeSyncMethod {
                    ffi_name: call.symbol.as_str().to_string(),
                    include_handle: method.receiver != Receiver::Static,
                    params: self
                        .visible_native_params(&call)
                        .into_iter()
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect(),
                    return_jni_type: self.jni_type_for_return(&call),
                }
            })
            .collect();
        let streams = class
            .streams
            .iter()
            .map(|stream| {
                let abi_stream = self.abi_stream(class, stream);
                KotlinNativeStream {
                    subscribe: abi_stream.subscribe.as_str().to_string(),
                    poll: abi_stream.poll.as_str().to_string(),
                    pop_batch: abi_stream.pop_batch.as_str().to_string(),
                    wait: abi_stream.wait.as_str().to_string(),
                    unsubscribe: abi_stream.unsubscribe.as_str().to_string(),
                    free: abi_stream.free.as_str().to_string(),
                }
            })
            .collect();
        KotlinNativeClass {
            ffi_free: naming::class_ffi_free(class.id.as_str()).into_string(),
            ctors,
            async_methods,
            sync_methods,
            streams,
        }
    }

    fn jni_type_for_return(&self, call: &AbiCall) -> String {
        self.jni_type_for_output(call.output_shape.output_binding())
    }

    fn jni_type_for_output(&self, output: OutputBinding<'_>) -> String {
        match output {
            OutputBinding::Unit => "Unit".to_string(),
            OutputBinding::Fast(FastOutputBinding::Scalar { abi_type }) => {
                self.jni_type_for_abi(&abi_type)
            }
            OutputBinding::Fast(_) | OutputBinding::Wire(_) => "ByteArray?".to_string(),
            OutputBinding::Handle { .. } | OutputBinding::CallbackHandle { .. } => {
                "Long".to_string()
            }
        }
    }

    fn jni_type_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 => "Byte".to_string(),
            AbiType::U8 => "Byte".to_string(),
            AbiType::I16 => "Short".to_string(),
            AbiType::U16 => "Short".to_string(),
            AbiType::I32 => "Int".to_string(),
            AbiType::U32 => "Int".to_string(),
            AbiType::I64 => "Long".to_string(),
            AbiType::U64 => "Long".to_string(),
            AbiType::ISize => "Long".to_string(),
            AbiType::USize => "Long".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer => "Long".to_string(),
            AbiType::Void => "Unit".to_string(),
        }
    }

    fn jni_param_mapping(&self, param: &AbiParam) -> JniParamMapping {
        match param_binding(param) {
            ParamBinding::Input(InputBinding::Scalar) => JniParamMapping {
                role: JniParamRole::Direct {
                    jni_type: self.jni_type_for_abi(&param.ffi_type),
                },
                len_companion: None,
            },
            ParamBinding::Input(InputBinding::Utf8Slice { len_param }) => JniParamMapping {
                role: JniParamRole::StringParam,
                len_companion: Some(len_param.clone()),
            },
            ParamBinding::Input(InputBinding::PrimitiveSlice {
                element_abi,
                len_param,
                ..
            }) => JniParamMapping {
                role: JniParamRole::Buffer {
                    jni_type: self.jni_buffer_type(&element_abi),
                },
                len_companion: Some(len_param.clone()),
            },
            ParamBinding::Input(InputBinding::WirePacket { len_param, .. }) => JniParamMapping {
                role: JniParamRole::Encoded,
                len_companion: Some(len_param.clone()),
            },
            ParamBinding::Input(InputBinding::Handle { nullable, .. }) => JniParamMapping {
                role: JniParamRole::Handle { nullable },
                len_companion: None,
            },
            ParamBinding::Input(InputBinding::CallbackHandle {
                callback_id,
                nullable,
                ..
            }) => JniParamMapping {
                role: JniParamRole::Callback {
                    callback_id: callback_id.clone(),
                    nullable,
                },
                len_companion: None,
            },
            ParamBinding::Input(InputBinding::OutputBuffer { len_param, .. }) => JniParamMapping {
                role: JniParamRole::OutBuffer,
                len_companion: Some(len_param.clone()),
            },
            ParamBinding::Hidden(_) | ParamBinding::UnsupportedValue => JniParamMapping {
                role: JniParamRole::Hidden,
                len_companion: None,
            },
        }
    }

    fn jni_type_for_param(&self, param: &AbiParam) -> String {
        self.jni_param_mapping(param).jni_type()
    }

    fn jni_buffer_type(&self, element_abi: &AbiType) -> String {
        match element_abi {
            AbiType::I32 | AbiType::U32 => "IntArray".to_string(),
            AbiType::I16 | AbiType::U16 => "ShortArray".to_string(),
            AbiType::I64 | AbiType::U64 => "LongArray".to_string(),
            AbiType::F32 => "FloatArray".to_string(),
            AbiType::F64 => "DoubleArray".to_string(),
            AbiType::U8 | AbiType::I8 => "ByteArray".to_string(),
            AbiType::Bool => "BooleanArray".to_string(),
            _ => "ByteBuffer".to_string(),
        }
    }

    fn kotlin_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => self.primitive_kotlin_type(*p),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Bytes => "ByteArray".to_string(),
            TypeExpr::Builtin(id) => self.builtin_kotlin_type(id),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Custom(id) => {
                if let Some(mapping) = self.type_mappings.get(id.as_str()) {
                    mapping.native_type.clone()
                } else {
                    NamingConvention::class_name(id.as_str())
                }
            }
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Vec(inner) => self.kotlin_vec_type(inner),
            TypeExpr::Option(inner) => format!("{}?", self.kotlin_type(inner)),
            TypeExpr::Result { ok, err } => {
                format!(
                    "BoltFFIResult<{}, {}>",
                    self.kotlin_type(ok),
                    self.kotlin_type(err)
                )
            }
            TypeExpr::Handle(class_id) => NamingConvention::class_name(class_id.as_str()),
            TypeExpr::Callback(callback_id) => NamingConvention::class_name(callback_id.as_str()),
            TypeExpr::Void => "Unit".to_string(),
        }
    }

    fn kotlin_type_with_disambiguation(
        &self,
        ty: &TypeExpr,
        reserved: &HashSet<String>,
    ) -> (String, Option<String>) {
        match ty {
            TypeExpr::Record(id) => self.disambiguate_type_name(id.as_str(), reserved),
            TypeExpr::Custom(id) => {
                if self.type_mappings.contains_key(id.as_str()) {
                    (self.kotlin_type(ty), None)
                } else {
                    self.disambiguate_type_name(id.as_str(), reserved)
                }
            }
            TypeExpr::Enum(id) => self.disambiguate_type_name(id.as_str(), reserved),
            _ => (self.kotlin_type(ty), None),
        }
    }

    fn qualify_decode_expr(&self, expr: String, qualified: Option<&str>) -> String {
        let Some(qualified) = qualified else {
            return expr;
        };
        let unqualified = qualified.rsplit('.').next().unwrap_or(qualified);
        let prefix = format!("{}.", unqualified);
        expr.strip_prefix(&prefix)
            .map(|suffix| format!("{}.{}", qualified, suffix))
            .unwrap_or(expr)
    }

    fn disambiguate_type_name(
        &self,
        type_name: &str,
        reserved: &HashSet<String>,
    ) -> (String, Option<String>) {
        let class_name = NamingConvention::class_name(type_name);
        if reserved.contains(&class_name) {
            let qualified = format!("{}.{}", self.package_name, class_name);
            (qualified.clone(), Some(qualified))
        } else {
            (class_name, None)
        }
    }

    fn kotlin_vec_type(&self, inner: &TypeExpr) -> String {
        match inner {
            TypeExpr::Primitive(p) => match p {
                PrimitiveType::I32 | PrimitiveType::U32 => "IntArray".to_string(),
                PrimitiveType::I16 | PrimitiveType::U16 => "ShortArray".to_string(),
                PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize => "LongArray".to_string(),
                PrimitiveType::F32 => "FloatArray".to_string(),
                PrimitiveType::F64 => "DoubleArray".to_string(),
                PrimitiveType::U8 | PrimitiveType::I8 => "ByteArray".to_string(),
                PrimitiveType::Bool => "BooleanArray".to_string(),
            },
            _ => format!("List<{}>", self.kotlin_type(inner)),
        }
    }

    fn primitive_kotlin_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 => "Byte".to_string(),
            PrimitiveType::U8 => "UByte".to_string(),
            PrimitiveType::I16 => "Short".to_string(),
            PrimitiveType::U16 => "UShort".to_string(),
            PrimitiveType::I32 => "Int".to_string(),
            PrimitiveType::U32 => "UInt".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "Long".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "ULong".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        }
    }

    fn primitive_jni_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 | PrimitiveType::U8 => "Byte".to_string(),
            PrimitiveType::I16 | PrimitiveType::U16 => "Short".to_string(),
            PrimitiveType::I32 | PrimitiveType::U32 => "Int".to_string(),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "Long".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        }
    }

    fn builtin_kotlin_type(&self, id: &BuiltinId) -> String {
        match id.as_str() {
            "Duration" => "Duration".to_string(),
            "SystemTime" => "Instant".to_string(),
            "Uuid" => "UUID".to_string(),
            "Url" => "URI".to_string(),
            _ => "String".to_string(),
        }
    }

    fn kotlin_return_type_from_def(
        &self,
        returns: &ReturnDef,
        transport: &OutputBinding,
    ) -> Option<String> {
        let base = match returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(self.kotlin_type(ty)),
            ReturnDef::Result { ok, .. } => match ok {
                TypeExpr::Void => Some("Unit".to_string()),
                _ => Some(self.kotlin_type(ok)),
            },
        };
        match transport {
            OutputBinding::Handle { nullable: true, .. }
            | OutputBinding::CallbackHandle { nullable: true, .. } => base.map(|ty| {
                if ty.ends_with('?') {
                    ty
                } else {
                    format!("{}?", ty)
                }
            }),
            _ => base,
        }
    }

    fn kotlin_type_from_return_def(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "Unit".to_string(),
            ReturnDef::Value(ty) => self.kotlin_type(ty),
            ReturnDef::Result { ok, .. } => match ok {
                TypeExpr::Void => "Unit".to_string(),
                _ => self.kotlin_type(ok),
            },
        }
    }

    fn closure_interface_name(&self, callback_id: &str) -> String {
        let signature = callback_id
            .strip_prefix("__Closure_")
            .unwrap_or(callback_id);
        format!("{}Callback", signature)
    }

    fn closure_param_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Record(_) => "java.nio.ByteBuffer".to_string(),
            _ => self.kotlin_type(ty),
        }
    }

    fn kotlin_type_from_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 => "Byte".to_string(),
            AbiType::U8 => "UByte".to_string(),
            AbiType::I16 => "Short".to_string(),
            AbiType::U16 => "UShort".to_string(),
            AbiType::I32 => "Int".to_string(),
            AbiType::U32 => "UInt".to_string(),
            AbiType::I64 => "Long".to_string(),
            AbiType::U64 => "ULong".to_string(),
            AbiType::ISize => "Long".to_string(),
            AbiType::USize => "ULong".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer => "Long".to_string(),
            AbiType::Void => "Unit".to_string(),
        }
    }

    fn return_type_from_decode_ops(&self, seq: &ReadSeq) -> String {
        let op = seq.ops.first().expect("decode op");
        match op {
            ReadOp::Primitive { primitive, .. } => self.primitive_kotlin_type(*primitive),
            ReadOp::String { .. } => "String".to_string(),
            ReadOp::Bytes { .. } => "ByteArray".to_string(),
            ReadOp::Builtin { id, .. } => self.builtin_kotlin_type(id),
            ReadOp::Record { id, .. } => NamingConvention::class_name(id.as_str()),
            ReadOp::Enum { id, .. } => NamingConvention::class_name(id.as_str()),
            ReadOp::Vec { element_type, .. } => self.kotlin_vec_type(element_type),
            ReadOp::Option { some, .. } => format!("{}?", self.return_type_from_decode_ops(some)),
            ReadOp::Result { ok, .. } => self.return_type_from_decode_ops(ok),
            ReadOp::Custom { id, .. } => NamingConvention::class_name(id.as_str()),
        }
    }

    fn kotlin_return_meta(&self, output_binding: &OutputBinding) -> KotlinReturnMeta {
        match output_binding {
            OutputBinding::Unit => KotlinReturnMeta {
                is_unit: true,
                is_direct: false,
                cast: String::new(),
            },
            OutputBinding::Fast(FastOutputBinding::Scalar { abi_type }) => KotlinReturnMeta {
                is_unit: false,
                is_direct: true,
                cast: self.kotlin_return_cast(abi_type),
            },
            OutputBinding::Handle { class_id, nullable } => KotlinReturnMeta {
                is_unit: false,
                is_direct: true,
                cast: self.kotlin_handle_return_cast(class_id, *nullable),
            },
            OutputBinding::CallbackHandle {
                callback_id,
                nullable,
            } => KotlinReturnMeta {
                is_unit: false,
                is_direct: true,
                cast: self.kotlin_callback_return_cast(callback_id, *nullable),
            },
            OutputBinding::Fast(_) | OutputBinding::Wire(_) => KotlinReturnMeta {
                is_unit: false,
                is_direct: false,
                cast: String::new(),
            },
        }
    }

    fn kotlin_handle_return_cast(&self, class_id: &ClassId, nullable: bool) -> String {
        let class_name = NamingConvention::class_name(class_id.as_str());
        if nullable {
            format!(".takeIf {{ it != 0L }}?.let {{ {}(it) }}", class_name)
        } else {
            format!(".let {{ {}(it) }}", class_name)
        }
    }

    fn kotlin_callback_return_cast(&self, callback_id: &CallbackId, nullable: bool) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!(".takeIf {{ it != 0L }}?.let {{ {}.create(it) }}", bridge)
        } else {
            format!(".let {{ {}.create(it) }}", bridge)
        }
    }

    fn kotlin_return_cast(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::U8 => ".toUByte()".to_string(),
            AbiType::U16 => ".toUShort()".to_string(),
            AbiType::U32 => ".toUInt()".to_string(),
            AbiType::U64 | AbiType::USize => ".toULong()".to_string(),
            _ => String::new(),
        }
    }

    fn is_throwing_return(&self, returns: &ReturnDef) -> bool {
        matches!(returns, ReturnDef::Result { .. })
    }

    fn error_type_name(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Result { err, .. } => match err {
                TypeExpr::Enum(id) if self.is_error_enum(id) => {
                    NamingConvention::class_name(id.as_str())
                }
                _ => "FfiException".to_string(),
            },
            _ => "FfiException".to_string(),
        }
    }

    fn err_to_throwable(&self, err: &TypeExpr) -> String {
        match err {
            TypeExpr::String => "FfiException(-1, err)".to_string(),
            TypeExpr::Enum(id) if self.is_error_enum(id) => "err".to_string(),
            _ => "FfiException(-1, \"Error: $err\")".to_string(),
        }
    }

    fn is_error_enum(&self, id: &EnumId) -> bool {
        self.contract
            .catalog
            .resolve_enum(id)
            .map(|enumeration| enumeration.is_error)
            .unwrap_or(false)
    }

    fn wire_writers_for_params(&self, call: &AbiCall) -> Vec<KotlinWireWriter> {
        call.params
            .iter()
            .filter_map(|param| {
                self.input_write_ops(param)
                    .map(|encode_ops| KotlinWireWriter {
                        binding_name: format!("wire_writer_{}", param.name.as_str()),
                        size_expr: emit::emit_size_expr_for_write_seq(&encode_ops),
                        encode_expr: emit::emit_write_expr(&encode_ops),
                    })
            })
            .collect()
    }

    fn native_arg_for_mapping(
        &self,
        param: &AbiParam,
        mapping: &JniParamMapping,
        writers: &[KotlinWireWriter],
    ) -> String {
        let name = NamingConvention::param_name(param.name.as_str());
        match &mapping.role {
            JniParamRole::Direct { .. } => match &param.ffi_type {
                AbiType::U64 | AbiType::USize => format!("{}.toLong()", name),
                AbiType::U32 => format!("{}.toInt()", name),
                AbiType::U16 => format!("{}.toShort()", name),
                AbiType::U8 => format!("{}.toByte()", name),
                _ => name,
            },
            JniParamRole::StringParam | JniParamRole::Buffer { .. } => name,
            JniParamRole::Encoded => writers
                .iter()
                .find(|w| w.binding_name == format!("wire_writer_{}", param.name.as_str()))
                .map(|w| format!("{}.buffer", w.binding_name))
                .unwrap_or_else(|| "wire.buffer".to_string()),
            JniParamRole::Handle { nullable } => {
                if *nullable {
                    format!("{}?.handle ?: 0L", name)
                } else {
                    format!("{}.handle", name)
                }
            }
            JniParamRole::Callback {
                callback_id,
                nullable,
            } => {
                let bridge = format!(
                    "{}Bridge",
                    NamingConvention::class_name(callback_id.as_str())
                );
                if *nullable {
                    format!("{}?.let {{ {}.create(it) }} ?: 0L", name, bridge)
                } else {
                    format!("{}.create({})", bridge, name)
                }
            }
            JniParamRole::OutBuffer => self.writer_pack_expr_for_param(param, &name),
            JniParamRole::Hidden => name,
        }
    }

    fn jni_param_mappings<'b>(&self, call: &'b AbiCall) -> Vec<(&'b AbiParam, JniParamMapping)> {
        call.params
            .iter()
            .map(|param| (param, self.jni_param_mapping(param)))
            .collect()
    }

    fn visible_native_params<'b>(&'b self, call: &'b AbiCall) -> Vec<&'b AbiParam> {
        let mappings = self.jni_param_mappings(call);
        let len_params: HashSet<&ParamName> = mappings
            .iter()
            .filter_map(|(_, m)| m.len_companion.as_ref())
            .collect();
        mappings
            .iter()
            .filter(|(param, mapping)| !len_params.contains(&param.name) && mapping.is_visible())
            .map(|(param, _)| *param)
            .collect()
    }

    fn native_args_for_params(&self, call: &AbiCall, writers: &[KotlinWireWriter]) -> Vec<String> {
        let mappings = self.jni_param_mappings(call);
        let len_params: HashSet<&ParamName> = mappings
            .iter()
            .filter_map(|(_, m)| m.len_companion.as_ref())
            .collect();
        mappings
            .iter()
            .filter(|(param, mapping)| !len_params.contains(&param.name) && mapping.is_visible())
            .map(|(param, mapping)| self.native_arg_for_mapping(param, mapping, writers))
            .collect()
    }

    fn is_instance_receiver(param: &AbiParam) -> bool {
        param.name.as_str() == "self"
            && matches!(
                param_binding(param),
                ParamBinding::Input(InputBinding::Handle { .. })
            )
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

    fn decode_expr_for_call_return(
        &self,
        returns: &OutputBinding,
        returns_def: &ReturnDef,
    ) -> String {
        if let Some(decode_ops) = returns.decode_ops() {
            if self.is_throwing_return(returns_def) {
                self.decode_result_expr(returns_def, decode_ops)
            } else if self.is_blittable_return(returns) {
                self.decode_blittable_return(decode_ops)
            } else {
                emit::emit_reader_read(decode_ops)
            }
        } else {
            match returns {
                OutputBinding::Unit | OutputBinding::Fast(FastOutputBinding::Scalar { .. }) => {
                    String::new()
                }
                OutputBinding::Handle { class_id, nullable } => {
                    self.decode_handle_return(class_id, *nullable, "result")
                }
                OutputBinding::CallbackHandle {
                    callback_id,
                    nullable,
                } => self.decode_callback_return(callback_id, *nullable, "result"),
                OutputBinding::Fast(_) | OutputBinding::Wire(_) => unreachable!(),
            }
        }
    }

    fn decode_result_expr(&self, returns: &ReturnDef, decode_ops: &ReadSeq) -> String {
        let (ok_seq, err_seq) = match decode_ops.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => (ok.as_ref(), err.as_ref()),
            _ => return emit::emit_reader_read(decode_ops),
        };
        let ok_expr = match returns {
            ReturnDef::Result {
                ok: TypeExpr::Void, ..
            } => "Unit".to_string(),
            _ => emit::emit_reader_read(ok_seq),
        };
        let err_expr = emit::emit_reader_read(err_seq);
        format!(
            "reader.readResult({{ {} }}, {{ {} }}).getOrThrow()",
            ok_expr, err_expr
        )
    }

    fn decode_handle_return(&self, class_id: &ClassId, nullable: bool, value_expr: &str) -> String {
        let class_name = NamingConvention::class_name(class_id.as_str());
        if nullable {
            format!(
                "{}.takeIf {{ it != 0L }}?.let {{ {}(it) }}",
                value_expr, class_name
            )
        } else {
            format!("{}({})", class_name, value_expr)
        }
    }

    fn decode_callback_return(
        &self,
        callback_id: &CallbackId,
        nullable: bool,
        value_expr: &str,
    ) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!(
                "{}.takeIf {{ it != 0L }}?.let {{ {}.create(it) }}",
                value_expr, bridge
            )
        } else {
            format!("{}.create({})", bridge, value_expr)
        }
    }

    fn is_blittable_return(&self, returns: &OutputBinding) -> bool {
        returns
            .decode_ops()
            .map(|decode_ops| self.is_blittable_decode_seq(decode_ops))
            .unwrap_or(false)
    }

    fn decode_blittable_return(&self, decode_ops: &ReadSeq) -> String {
        match decode_ops.ops.first() {
            Some(ReadOp::Record { id, .. }) => {
                format!(
                    "{}Reader.read(buffer, 0)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            Some(ReadOp::Vec {
                element_type: TypeExpr::Record(id),
                ..
            }) => format!(
                "{}Reader.readAll(buffer, 4, buffer.getInt(0))",
                NamingConvention::class_name(id.as_str())
            ),
            _ => emit::emit_reader_read(decode_ops),
        }
    }

    fn async_call_for_method(
        &self,
        _class: &ClassDef,
        method: &MethodDef,
        call: &AbiCall,
    ) -> KotlinAsyncCall {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => unreachable!("async method missing async call"),
        };
        let result_route = async_output_binding(async_call);
        let return_meta = self.kotlin_return_meta(&result_route);
        let decode_expr = self.decode_expr_for_call_return(&result_route, &method.returns);
        let is_blittable_return = self.is_blittable_return(&result_route);
        KotlinAsyncCall {
            poll: async_call.poll.as_str().to_string(),
            complete: async_call.complete.as_str().to_string(),
            cancel: async_call.cancel.as_str().to_string(),
            free: async_call.free.as_str().to_string(),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            return_cast: return_meta.cast,
            decode_expr,
            is_blittable_return,
        }
    }

    fn is_blittable_decode_seq(&self, decode_ops: &ReadSeq) -> bool {
        match decode_ops.ops.first() {
            Some(ReadOp::Record { id, .. }) => self
                .contract
                .catalog
                .resolve_record(id)
                .map(|record| record.is_blittable())
                .unwrap_or(false),
            Some(ReadOp::Vec {
                element_type,
                layout,
                ..
            }) => {
                matches!(layout, VecLayout::Blittable { .. })
                    && matches!(element_type, TypeExpr::Record(_))
            }
            _ => false,
        }
    }

    fn async_call_for_function(&self, func: &FunctionDef, call: &AbiCall) -> KotlinAsyncCall {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => unreachable!("async function missing async call"),
        };
        let result_route = async_output_binding(async_call);
        let return_meta = self.kotlin_return_meta(&result_route);
        let decode_expr = self.decode_expr_for_call_return(&result_route, &func.returns);
        let is_blittable_return = self.is_blittable_return(&result_route);
        KotlinAsyncCall {
            poll: async_call.poll.as_str().to_string(),
            complete: async_call.complete.as_str().to_string(),
            cancel: async_call.cancel.as_str().to_string(),
            free: async_call.free.as_str().to_string(),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            return_cast: return_meta.cast,
            decode_expr,
            is_blittable_return,
        }
    }

    fn record_struct_size(&self, record_id: &str) -> usize {
        self.abi
            .records
            .iter()
            .find(|record| record.id.as_str() == record_id)
            .and_then(|record| record.size)
            .unwrap_or(0)
    }

    fn field_padding_after(&self, record_id: &RecordId, field_name: &FieldName) -> usize {
        let record = match self.abi_record_for(record_id) {
            Some(record) if record.is_blittable => record,
            _ => return 0,
        };

        let fields = match self.record_field_offsets(record) {
            Some(fields) => fields,
            None => return 0,
        };
        let current = match fields.iter().find(|field| field.name == *field_name) {
            Some(field) => field,
            None => return 0,
        };
        let next_offset = fields
            .iter()
            .filter(|field| field.offset > current.offset)
            .map(|field| field.offset)
            .min()
            .unwrap_or(record.size.unwrap_or(0));

        next_offset.saturating_sub(current.offset + current.size)
    }

    fn should_generate_fixed_enum_codec(&self, enumeration: &EnumDef) -> bool {
        match &enumeration.repr {
            EnumRepr::Data { variants, .. } => {
                variants.iter().all(|variant| match &variant.payload {
                    VariantPayload::Unit => true,
                    VariantPayload::Struct(fields) => fields
                        .iter()
                        .all(|field| matches!(field.type_expr, TypeExpr::Primitive(_))),
                    VariantPayload::Tuple(fields) => {
                        fields.iter().all(|ty| matches!(ty, TypeExpr::Primitive(_)))
                    }
                })
            }
            _ => false,
        }
    }

    fn data_enum_layout(&self, enumeration: &EnumDef) -> Option<DataEnumLayout> {
        let EnumRepr::Data { variants, .. } = &enumeration.repr else {
            return None;
        };

        let tag_size = 4usize;
        let tag_alignment = 4usize;

        let variant_layouts = variants
            .iter()
            .map(|variant| self.data_enum_variant_layout(&variant.payload))
            .collect::<Vec<_>>();

        let union_alignment = variant_layouts
            .iter()
            .map(|layout| layout.alignment)
            .max()
            .unwrap_or(1);

        let union_size = variant_layouts
            .iter()
            .map(|layout| layout.size)
            .max()
            .unwrap_or(0);

        let payload_offset = align_up(tag_size, union_alignment);
        let struct_alignment = tag_alignment.max(union_alignment);
        let struct_size = align_up(
            payload_offset + align_up(union_size, union_alignment),
            struct_alignment,
        );

        Some(DataEnumLayout {
            struct_size,
            payload_offset,
            variant_offsets: variant_layouts
                .into_iter()
                .map(|layout| layout.offsets)
                .collect(),
        })
    }

    fn data_enum_variant_layout(&self, payload: &VariantPayload) -> DataEnumVariantLayout {
        match payload {
            VariantPayload::Unit => DataEnumVariantLayout {
                offsets: Vec::new(),
                size: 0,
                alignment: 1,
            },
            VariantPayload::Struct(fields) => {
                let primitives = fields
                    .iter()
                    .map(|field| match field.type_expr {
                        TypeExpr::Primitive(primitive) => primitive,
                        _ => panic!("data enum payload must be primitive"),
                    })
                    .collect::<Vec<_>>();
                self.primitive_fields_layout(&primitives)
            }
            VariantPayload::Tuple(fields) => {
                let primitives = fields
                    .iter()
                    .map(|ty| match ty {
                        TypeExpr::Primitive(primitive) => *primitive,
                        _ => panic!("data enum payload must be primitive"),
                    })
                    .collect::<Vec<_>>();
                self.primitive_fields_layout(&primitives)
            }
        }
    }

    fn primitive_fields_layout(&self, primitives: &[PrimitiveType]) -> DataEnumVariantLayout {
        let (offsets, size, alignment) = primitives.iter().fold(
            (Vec::new(), 0usize, 1usize),
            |(mut offsets, mut current, mut alignment), primitive| {
                let (size, align) = primitive_layout(*primitive);
                let aligned = align_up(current, align);
                offsets.push(aligned);
                current = aligned + size;
                alignment = alignment.max(align);
                (offsets, current, alignment)
            },
        );
        DataEnumVariantLayout {
            offsets,
            size: align_up(size, alignment),
            alignment,
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

    fn record_field_offsets(&self, record: &AbiRecord) -> Option<Vec<RecordFieldOffset>> {
        match record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|field| {
                    let offset = read_seq_offset(&field.seq)?;
                    let size = match &field.seq.size {
                        SizeExpr::Fixed(value) => *value,
                        _ => return None,
                    };
                    Some(RecordFieldOffset {
                        name: field.name.clone(),
                        offset,
                        size,
                    })
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }

    fn record_blittable_fields(&self, record_id: &RecordId) -> Option<Vec<RecordBlittableField>> {
        let record = self.abi_record_for(record_id)?;
        if !record.is_blittable {
            return None;
        }
        match record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|field| match field.seq.ops.first() {
                    Some(ReadOp::Primitive { primitive, .. }) => {
                        read_seq_offset(&field.seq).map(|offset_value| RecordBlittableField {
                            name: field.name.clone(),
                            offset: offset_value,
                            primitive: *primitive,
                        })
                    }
                    _ => None,
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn abi_enum_for(&self, enumeration: &EnumDef) -> &AbiEnum {
        self.abi
            .enums
            .iter()
            .find(|abi_enum| abi_enum.id == enumeration.id)
            .expect("abi enum missing")
    }

    fn abi_call_for_function(&self, function: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| call.id == CallId::Function(function.id.clone()))
            .expect("abi call missing for function")
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| {
                call.id
                    == CallId::Method {
                        class_id: class.id.clone(),
                        method_id: method.id.clone(),
                    }
            })
            .expect("abi call missing for method")
    }

    fn abi_call_for_constructor(&self, class: &ClassDef, index: usize) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| {
                call.id
                    == CallId::Constructor {
                        class_id: class.id.clone(),
                        index,
                    }
            })
            .expect("abi call missing for constructor")
    }

    fn abi_callback_for(&self, callback_id: &CallbackId) -> &AbiCallbackInvocation {
        self.abi
            .callbacks
            .iter()
            .find(|callback| callback.callback_id == *callback_id)
            .expect("abi callback missing")
    }

    fn abi_callback_method(
        &self,
        callback_id: &CallbackId,
        method_id: &MethodId,
    ) -> &AbiCallbackMethod {
        self.abi_callback_for(callback_id)
            .methods
            .iter()
            .find(|method| method.id == *method_id)
            .expect("abi callback method missing")
    }

    fn primitive_field_accessors(&self, primitive: PrimitiveType) -> (String, String, String) {
        let getter = match primitive {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => "get",
            PrimitiveType::I16 | PrimitiveType::U16 => "getShort",
            PrimitiveType::I32 | PrimitiveType::U32 => "getInt",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "getLong",
            PrimitiveType::F32 => "getFloat",
            PrimitiveType::F64 => "getDouble",
        }
        .to_string();

        let putter = match primitive {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => "put",
            PrimitiveType::I16 | PrimitiveType::U16 => "putShort",
            PrimitiveType::I32 | PrimitiveType::U32 => "putInt",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "putLong",
            PrimitiveType::F32 => "putFloat",
            PrimitiveType::F64 => "putDouble",
        }
        .to_string();

        let conversion = match primitive {
            PrimitiveType::Bool => " != 0.toByte()",
            PrimitiveType::U8 => ".toUByte()",
            PrimitiveType::U16 => ".toUShort()",
            PrimitiveType::U32 => ".toUInt()",
            PrimitiveType::U64 | PrimitiveType::USize => ".toULong()",
            _ => "",
        }
        .to_string();

        (getter, putter, conversion)
    }

    fn primitive_write_value_expr(&self, primitive: PrimitiveType, value: &str) -> String {
        match primitive {
            PrimitiveType::Bool => format!("(if ({}) 1 else 0).toByte()", value),
            PrimitiveType::U8 => format!("({}).toByte()", value),
            PrimitiveType::U16 => format!("({}).toShort()", value),
            PrimitiveType::U32 => format!("({}).toInt()", value),
            PrimitiveType::U64 | PrimitiveType::USize => format!("({}).toLong()", value),
            _ => value.to_string(),
        }
    }

    fn find_custom_read_seq(&self, custom: &CustomTypeId) -> Option<ReadSeq> {
        self.read_seqs()
            .into_iter()
            .find_map(|seq| self.read_seq_custom(&seq, custom))
    }

    fn read_seqs(&self) -> Vec<ReadSeq> {
        let record_seqs = self
            .abi
            .records
            .iter()
            .map(|record| record.decode_ops.clone());
        let enum_seqs = self
            .abi
            .enums
            .iter()
            .map(|enumeration| enumeration.decode_ops.clone());
        let enum_field_seqs = self.abi.enums.iter().flat_map(|enumeration| {
            enumeration
                .variants
                .iter()
                .flat_map(|variant| match &variant.payload {
                    AbiEnumPayload::Unit => Vec::new(),
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                        fields.iter().map(|field| field.decode.clone()).collect()
                    }
                })
        });
        let call_seqs = self.abi.calls.iter().flat_map(|call| {
            let return_seq = self.output_read_ops(&call.output_shape);
            let param_seqs = call
                .params
                .iter()
                .filter_map(|param| self.input_read_ops(param));
            let error_seq = match &call.error {
                ErrorTransport::Encoded { decode_ops, .. } => Some(decode_ops),
                ErrorTransport::None | ErrorTransport::StatusCode => None,
            };
            let async_seq = match &call.mode {
                CallMode::Async(async_call) => self.output_read_ops(&async_call.result_shape),
                CallMode::Sync => None,
            };
            return_seq
                .into_iter()
                .chain(param_seqs)
                .chain(error_seq.cloned())
                .chain(async_seq)
        });
        let callback_seqs = self.abi.callbacks.iter().flat_map(|callback| {
            callback.methods.iter().flat_map(|method| {
                let return_seq = self.output_read_ops(&method.output_shape);
                let param_seqs = method
                    .params
                    .iter()
                    .filter_map(|param| self.input_read_ops(param));
                return_seq.into_iter().chain(param_seqs)
            })
        });

        record_seqs
            .chain(enum_seqs)
            .chain(enum_field_seqs)
            .chain(call_seqs)
            .chain(callback_seqs)
            .collect()
    }

    fn read_seq_custom(&self, seq: &ReadSeq, custom: &CustomTypeId) -> Option<ReadSeq> {
        seq.ops.iter().find_map(|op| match op {
            ReadOp::Custom { id, underlying } if id == custom => Some(*underlying.clone()),
            ReadOp::Option { some, .. } => self.read_seq_custom(some, custom),
            ReadOp::Vec { element, .. } => self.read_seq_custom(element, custom),
            ReadOp::Record { fields, .. } => fields
                .iter()
                .find_map(|field| self.read_seq_custom(&field.seq, custom)),
            ReadOp::Result { ok, err, .. } => self
                .read_seq_custom(ok, custom)
                .or_else(|| self.read_seq_custom(err, custom)),
            _ => None,
        })
    }

    fn find_custom_write_seq(&self, custom: &CustomTypeId) -> Option<WriteSeq> {
        self.write_seqs()
            .into_iter()
            .find_map(|seq| self.write_seq_custom(&seq, custom))
    }

    fn write_seqs(&self) -> Vec<WriteSeq> {
        let record_seqs = self
            .abi
            .records
            .iter()
            .map(|record| record.encode_ops.clone());
        let enum_seqs = self
            .abi
            .enums
            .iter()
            .map(|enumeration| enumeration.encode_ops.clone());
        let enum_field_seqs = self.abi.enums.iter().flat_map(|enumeration| {
            enumeration
                .variants
                .iter()
                .flat_map(|variant| match &variant.payload {
                    AbiEnumPayload::Unit => Vec::new(),
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                        fields.iter().map(|field| field.encode.clone()).collect()
                    }
                })
        });
        let call_seqs = self.abi.calls.iter().flat_map(|call| {
            let return_seq = self.output_write_ops(&call.output_shape);
            let param_seqs = call
                .params
                .iter()
                .filter_map(|param| self.input_write_ops(param));
            let async_seq = match &call.mode {
                CallMode::Async(async_call) => self.output_write_ops(&async_call.result_shape),
                CallMode::Sync => None,
            };
            return_seq.into_iter().chain(param_seqs).chain(async_seq)
        });
        let callback_seqs = self.abi.callbacks.iter().flat_map(|callback| {
            callback.methods.iter().flat_map(|method| {
                let return_seq = self.output_write_ops(&method.output_shape);
                let param_seqs = method
                    .params
                    .iter()
                    .filter_map(|param| self.input_write_ops(param));
                return_seq.into_iter().chain(param_seqs)
            })
        });

        record_seqs
            .chain(enum_seqs)
            .chain(enum_field_seqs)
            .chain(call_seqs)
            .chain(callback_seqs)
            .collect()
    }

    fn write_seq_custom(&self, seq: &WriteSeq, custom: &CustomTypeId) -> Option<WriteSeq> {
        seq.ops.iter().find_map(|op| match op {
            WriteOp::Custom { id, underlying, .. } if id == custom => Some(*underlying.clone()),
            WriteOp::Option { some, .. } => self.write_seq_custom(some, custom),
            WriteOp::Vec { element, .. } => self.write_seq_custom(element, custom),
            WriteOp::Record { fields, .. } => fields
                .iter()
                .find_map(|field| self.write_seq_custom(&field.seq, custom)),
            WriteOp::Result { ok, err, .. } => self
                .write_seq_custom(ok, custom)
                .or_else(|| self.write_seq_custom(err, custom)),
            _ => None,
        })
    }

    fn read_seq_from_repr(&self, repr: &TypeExpr) -> ReadSeq {
        self.find_read_seq_for_type(repr)
            .unwrap_or_else(|| panic!("missing read ops for custom repr: {:?}", repr))
    }

    fn write_seq_from_repr(&self, repr: &TypeExpr) -> WriteSeq {
        self.find_write_seq_for_type(repr)
            .unwrap_or_else(|| panic!("missing write ops for custom repr: {:?}", repr))
    }

    fn find_read_seq_for_type(&self, ty: &TypeExpr) -> Option<ReadSeq> {
        self.read_seqs()
            .into_iter()
            .find(|seq| self.read_seq_matches_type(seq, ty))
    }

    fn find_write_seq_for_type(&self, ty: &TypeExpr) -> Option<WriteSeq> {
        self.write_seqs()
            .into_iter()
            .find(|seq| self.write_seq_matches_type(seq, ty))
    }

    fn read_seq_matches_type(&self, seq: &ReadSeq, ty: &TypeExpr) -> bool {
        match (seq.ops.first(), ty) {
            (Some(ReadOp::Primitive { primitive, .. }), TypeExpr::Primitive(expected)) => {
                primitive == expected
            }
            (Some(ReadOp::String { .. }), TypeExpr::String) => true,
            (Some(ReadOp::Bytes { .. }), TypeExpr::Bytes) => true,
            (Some(ReadOp::Builtin { id, .. }), TypeExpr::Builtin(expected)) => id == expected,
            (Some(ReadOp::Record { id, .. }), TypeExpr::Record(expected)) => id == expected,
            (Some(ReadOp::Enum { id, .. }), TypeExpr::Enum(expected)) => id == expected,
            (Some(ReadOp::Custom { id, .. }), TypeExpr::Custom(expected)) => id == expected,
            (Some(ReadOp::Vec { element_type, .. }), TypeExpr::Vec(inner)) => {
                element_type == inner.as_ref()
            }
            (Some(ReadOp::Option { some, .. }), TypeExpr::Option(inner)) => {
                self.read_seq_matches_type(some, inner)
            }
            (
                Some(ReadOp::Result { ok, err, .. }),
                TypeExpr::Result {
                    ok: ok_ty,
                    err: err_ty,
                },
            ) => self.read_seq_matches_type(ok, ok_ty) && self.read_seq_matches_type(err, err_ty),
            _ => false,
        }
    }

    fn write_seq_matches_type(&self, seq: &WriteSeq, ty: &TypeExpr) -> bool {
        match (seq.ops.first(), ty) {
            (Some(WriteOp::Primitive { primitive, .. }), TypeExpr::Primitive(expected)) => {
                primitive == expected
            }
            (Some(WriteOp::String { .. }), TypeExpr::String) => true,
            (Some(WriteOp::Bytes { .. }), TypeExpr::Bytes) => true,
            (Some(WriteOp::Builtin { id, .. }), TypeExpr::Builtin(expected)) => id == expected,
            (Some(WriteOp::Record { id, .. }), TypeExpr::Record(expected)) => id == expected,
            (Some(WriteOp::Enum { id, .. }), TypeExpr::Enum(expected)) => id == expected,
            (Some(WriteOp::Custom { id, .. }), TypeExpr::Custom(expected)) => id == expected,
            (Some(WriteOp::Vec { element_type, .. }), TypeExpr::Vec(inner)) => {
                element_type == inner.as_ref()
            }
            (Some(WriteOp::Option { some, .. }), TypeExpr::Option(inner)) => {
                self.write_seq_matches_type(some, inner)
            }
            (
                Some(WriteOp::Result { ok, err, .. }),
                TypeExpr::Result {
                    ok: ok_ty,
                    err: err_ty,
                },
            ) => self.write_seq_matches_type(ok, ok_ty) && self.write_seq_matches_type(err, err_ty),
            _ => false,
        }
    }

    fn blittable_return_record_ids(&self) -> HashSet<String> {
        let sync_returns = self.abi.calls.iter().filter_map(|call| {
            self.output_read_ops(&call.output_shape)
                .and_then(|seq| self.blittable_record_id_from_read_seq(&seq))
        });

        let async_returns = self.abi.calls.iter().filter_map(|call| match &call.mode {
            CallMode::Async(async_call) => self
                .output_read_ops(&async_call.result_shape)
                .and_then(|seq| self.blittable_record_id_from_read_seq(&seq)),
            CallMode::Sync => None,
        });

        sync_returns.chain(async_returns).collect()
    }

    fn blittable_record_from_decode_ops(&self, transport: &OutputBinding) -> Option<String> {
        transport
            .decode_ops()
            .and_then(|decode_ops| self.blittable_record_id_from_read_seq(decode_ops))
    }

    fn blittable_record_id_from_read_seq(&self, seq: &ReadSeq) -> Option<String> {
        match seq.ops.first() {
            Some(ReadOp::Record { id, .. }) if self.is_record_blittable(id.as_str()) => {
                Some(id.as_str().to_string())
            }
            Some(ReadOp::Vec {
                element_type: TypeExpr::Record(id),
                layout,
                ..
            }) if matches!(layout, VecLayout::Blittable { .. })
                && self.is_record_blittable(id.as_str()) =>
            {
                Some(id.as_str().to_string())
            }
            _ => None,
        }
    }

    fn is_record_blittable(&self, record_id: &str) -> bool {
        self.contract
            .catalog
            .resolve_record(&RecordId::new(record_id))
            .map(|record| record.is_blittable())
            .unwrap_or(false)
    }

    fn blittable_vec_param_records(&self) -> HashSet<String> {
        let types_from_functions = self
            .contract
            .functions
            .iter()
            .flat_map(|function| function.params.iter())
            .map(|param| &param.type_expr);
        let types_from_methods = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.methods.iter())
            .flat_map(|method| method.params.iter())
            .map(|param| &param.type_expr);
        let types_from_ctors = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.constructors.iter())
            .flat_map(|ctor| ctor.params().into_iter())
            .map(|param| &param.type_expr);
        let types_from_traits = self
            .contract
            .catalog
            .all_callbacks()
            .flat_map(|callback| callback.methods.iter())
            .flat_map(|method| method.params.iter())
            .map(|param| &param.type_expr);
        let types_from_records = self
            .contract
            .catalog
            .all_records()
            .flat_map(|record| record.fields.iter())
            .map(|field| &field.type_expr);
        let types_from_enums =
            self.contract
                .catalog
                .all_enums()
                .flat_map(|enumeration| match &enumeration.repr {
                    EnumRepr::Data { variants, .. } => variants
                        .iter()
                        .flat_map(|variant| match &variant.payload {
                            VariantPayload::Struct(fields) => fields
                                .iter()
                                .map(|field| &field.type_expr)
                                .collect::<Vec<_>>(),
                            VariantPayload::Tuple(fields) => fields.iter().collect::<Vec<_>>(),
                            VariantPayload::Unit => Vec::new(),
                        })
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                });

        types_from_functions
            .chain(types_from_methods)
            .chain(types_from_ctors)
            .chain(types_from_traits)
            .chain(types_from_records)
            .chain(types_from_enums)
            .filter_map(|ty| match ty {
                TypeExpr::Vec(inner) => match inner.as_ref() {
                    TypeExpr::Record(id) => Some(id.as_str().to_string()),
                    _ => None,
                },
                _ => None,
            })
            .filter(|record_name| {
                self.contract
                    .catalog
                    .all_records()
                    .any(|record| record.id.as_str() == *record_name && record.is_blittable())
            })
            .collect()
    }

    fn writer_pack_expr_for_param(&self, param: &AbiParam, kotlin_name: &str) -> String {
        let record_id = self.out_buffer_record_id(param);
        match record_id {
            Some(id) => format!(
                "{}Writer.pack({})",
                NamingConvention::class_name(&id),
                kotlin_name,
            ),
            None => kotlin_name.to_string(),
        }
    }

    fn out_buffer_record_id(&self, param: &AbiParam) -> Option<String> {
        match param.input_binding() {
            Some(InputBinding::OutputBuffer { decode_ops, .. }) => match decode_ops.ops.first() {
                Some(ReadOp::Vec {
                    element_type: TypeExpr::Record(id),
                    ..
                }) => Some(id.as_str().to_string()),
                Some(ReadOp::Record { id, .. }) => Some(id.as_str().to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    fn input_read_ops(&self, param: &AbiParam) -> Option<ReadSeq> {
        match param.input_binding() {
            Some(InputBinding::WirePacket { decode_ops, .. }) => Some(decode_ops.clone()),
            Some(InputBinding::OutputBuffer { decode_ops, .. }) => Some(decode_ops.clone()),
            _ => None,
        }
    }

    fn input_write_ops(&self, param: &AbiParam) -> Option<WriteSeq> {
        match param.input_binding() {
            Some(InputBinding::WirePacket { encode_ops, .. }) => Some(encode_ops.clone()),
            _ => None,
        }
    }

    fn output_read_ops(&self, output_shape: &OutputShape) -> Option<ReadSeq> {
        output_shape.output_binding().decode_ops().cloned()
    }

    fn output_write_ops(&self, output_shape: &OutputShape) -> Option<WriteSeq> {
        output_shape.output_binding().encode_ops().cloned()
    }
}

struct RecordFieldOffset {
    name: FieldName,
    offset: usize,
    size: usize,
}

struct RecordBlittableField {
    name: FieldName,
    offset: usize,
    primitive: PrimitiveType,
}

struct DataEnumLayout {
    struct_size: usize,
    payload_offset: usize,
    variant_offsets: Vec<Vec<usize>>,
}

struct DataEnumVariantLayout {
    offsets: Vec<usize>,
    size: usize,
    alignment: usize,
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

fn primitive_layout(primitive: PrimitiveType) -> (usize, usize) {
    let size = primitive.wire_size_bytes();
    let alignment = match primitive {
        PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => 1,
        PrimitiveType::I16 | PrimitiveType::U16 => 2,
        PrimitiveType::I32 | PrimitiveType::U32 | PrimitiveType::F32 => 4,
        PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize
        | PrimitiveType::F64 => 8,
    };
    (size, alignment)
}

fn read_seq_offset(seq: &ReadSeq) -> Option<usize> {
    let op = seq.ops.first()?;
    let offset = match op {
        ReadOp::Primitive { offset, .. }
        | ReadOp::String { offset }
        | ReadOp::Bytes { offset }
        | ReadOp::Builtin { offset, .. }
        | ReadOp::Record { offset, .. }
        | ReadOp::Enum { offset, .. } => offset,
        _ => return None,
    };
    match offset {
        OffsetExpr::Fixed(value) => Some(*value),
        OffsetExpr::Base => Some(0),
        OffsetExpr::BasePlus(value) => Some(*value),
        _ => None,
    }
}

fn kotlin_default_literal(default: &DefaultValue, kotlin_type: &str) -> String {
    use heck::ToUpperCamelCase;
    match default {
        DefaultValue::Bool(true) => "true".to_string(),
        DefaultValue::Bool(false) => "false".to_string(),
        DefaultValue::Integer(v) => match kotlin_type {
            "Double" => format!("{}.0", v),
            "Float" => format!("{}.0f", v),
            "UInt" => format!("{}u", v),
            "ULong" => format!("{}uL", v),
            "UShort" => format!("{}u", v),
            "UByte" => format!("{}u", v),
            "Long" => format!("{}L", v),
            _ => v.to_string(),
        },
        DefaultValue::Float(v) => {
            let has_decimal = v.fract() != 0.0;
            let base = if has_decimal {
                format!("{}", v)
            } else {
                format!("{}.0", v)
            };
            match kotlin_type {
                "Float" => format!("{}f", base),
                _ => base,
            }
        }
        DefaultValue::String(v) => format!("\"{}\"", v),
        DefaultValue::EnumVariant {
            enum_name,
            variant_name,
        } => format!(
            "{}.{}",
            enum_name.to_upper_camel_case(),
            NamingConvention::enum_entry_name(variant_name)
        ),
        DefaultValue::Null => "null".to_string(),
    }
}

struct KotlinPreamble {
    prefix: String,
    extra_imports: Vec<String>,
    custom_types: Vec<KotlinCustomType>,
    has_streams: bool,
}

enum JniParamRole {
    Direct {
        jni_type: String,
    },
    StringParam,
    Buffer {
        jni_type: String,
    },
    Encoded,
    Handle {
        nullable: bool,
    },
    Callback {
        callback_id: CallbackId,
        nullable: bool,
    },
    OutBuffer,
    Hidden,
}

struct JniParamMapping {
    role: JniParamRole,
    len_companion: Option<ParamName>,
}

impl JniParamMapping {
    fn is_visible(&self) -> bool {
        !matches!(self.role, JniParamRole::Hidden)
    }

    fn jni_type(&self) -> String {
        match &self.role {
            JniParamRole::Direct { jni_type } | JniParamRole::Buffer { jni_type } => {
                jni_type.clone()
            }
            JniParamRole::StringParam => "String".to_string(),
            JniParamRole::Encoded | JniParamRole::OutBuffer => "ByteBuffer".to_string(),
            JniParamRole::Handle { .. } | JniParamRole::Callback { .. } => "Long".to_string(),
            JniParamRole::Hidden => "Unit".to_string(),
        }
    }
}
