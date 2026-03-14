use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::classification::{self, PassableCategory};
use boltffi_ffi_rules::naming;

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, AsyncCall, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnShape, StreamItemTransport,
};
use crate::ir::codec::{
    BlittableField, CodecPlan, EncodedField, EnumLayout, RecordLayout, VariantLayout,
    VariantPayloadLayout, VecLayout,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, EnumDef, EnumRepr, FunctionDef,
    MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef, StreamDef, VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId,
};
use crate::ir::ops::{
    FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape,
    WriteOp, WriteSeq,
};
use crate::ir::plan::{
    AbiType, AsyncPlan, CallPlan, CallPlanKind, CallTarget, CallbackStyle, CompletionCallback,
    CompositeField, CompositeLayout, Mutability, ParamPlan, ReturnPlan, ScalarOrigin, SpanContent,
    Transport,
};
use crate::ir::types::{PrimitiveType, TypeExpr};

#[derive(Debug, Clone)]
struct AbiCallbackParamPlan {
    name: ParamName,
    strategy: AbiCallbackParamStrategy,
}

#[derive(Debug, Clone)]
enum AbiCallbackParamStrategy {
    Scalar(PrimitiveType),
    Direct(CompositeLayout),
    Encoded { codec: CodecPlan },
}

fn return_shape_from_transport_with_ops(
    transport: Transport,
    decode_ops: ReadSeq,
    encode_ops: WriteSeq,
) -> ReturnShape {
    ReturnShape {
        transport: Some(transport),
        decode_ops: Some(decode_ops),
        encode_ops: Some(encode_ops),
    }
}

/// Walks an [`FfiContract`] and produces an [`AbiContract`].
///
/// Most of the work is codec planning, figuring out which records are blittable,
/// which enums are C-style vs data-carrying, and detecting recursive types.
/// `record_stack` and `enum_stack` track what we are currently lowering so we
/// catch cycles: if lowering `TreeNode` hits `TreeNode` again in its own fields,
/// that is a recursive type, and it gets encoded layout because a fixed size
/// does not exist.
pub struct Lowerer<'c> {
    contract: &'c FfiContract,
    // tracks which records and enums we are currently lowering so we detect cycles.
    // if we hit the same id again mid-walk, the type is recursive and gets
    // encoded layout instead of blittable.
    record_stack: RefCell<HashSet<RecordId>>,
    enum_stack: RefCell<HashSet<EnumId>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn new(contract: &'c FfiContract) -> Self {
        Self {
            contract,
            record_stack: RefCell::new(HashSet::new()),
            enum_stack: RefCell::new(HashSet::new()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ABI Contract Generation
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn to_abi_contract(&self) -> AbiContract {
        let function_calls = self
            .contract
            .functions
            .iter()
            .map(|func| self.abi_call_for_function(func));

        let class_calls = self.contract.catalog.all_classes().flat_map(|class| {
            let ctor_calls = class
                .constructors
                .iter()
                .enumerate()
                .map(|(index, ctor)| self.abi_call_for_constructor(class, ctor, index));
            let method_calls = class
                .methods
                .iter()
                .map(|method| self.abi_call_for_method(class, method));
            ctor_calls.chain(method_calls)
        });

        let calls = function_calls.chain(class_calls).collect();

        let callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .map(|callback| self.abi_callback_invocation(callback))
            .collect();

        let records = self
            .contract
            .catalog
            .all_records()
            .map(|record| self.abi_record(record))
            .collect();

        let enums = self
            .contract
            .catalog
            .all_enums()
            .map(|enumeration| self.abi_enum(enumeration))
            .collect();

        let streams = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| {
                class
                    .streams
                    .iter()
                    .map(|stream| self.abi_stream(&class.id, stream))
            })
            .collect();

        AbiContract {
            package: self.contract.package.clone(),
            calls,
            callbacks,
            streams,
            records,
            enums,
            free_buf: naming::free_buf(),
            atomic_cas: naming::atomic_u8_cas(),
        }
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> AbiCall {
        let plan = self.lower_function(func);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (mode, returns, error) = self.abi_mode_returns_error_for_function(func, &plan.kind);

        AbiCall {
            id: CallId::Function(func.id.clone()),
            symbol,
            mode,
            params,
            returns,
            error,
        }
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> AbiCall {
        let plan = self.lower_method(class, method);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (mode, returns, error) =
            self.abi_mode_returns_error_for_method(class, method, &plan.kind);

        AbiCall {
            id: CallId::Method {
                class_id: class.id.clone(),
                method_id: method.id.clone(),
            },
            symbol,
            mode,
            params,
            returns,
            error,
        }
    }

    fn abi_call_for_constructor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        index: usize,
    ) -> AbiCall {
        let plan = self.lower_constructor(class, ctor);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (returns, error) = self.return_shape_and_error(match &plan.kind {
            CallPlanKind::Sync { returns } => returns,
            CallPlanKind::Async { .. } => panic!("constructors cannot be async"),
        });

        AbiCall {
            id: CallId::Constructor {
                class_id: class.id.clone(),
                index,
            },
            symbol,
            mode: CallMode::Sync,
            params,
            returns,
            error,
        }
    }

    fn abi_callback_invocation(&self, callback: &CallbackTraitDef) -> AbiCallbackInvocation {
        let methods = callback
            .methods
            .iter()
            .map(|method| {
                let params = self.abi_callback_params(callback, method).collect();
                let (returns, error) = self.callback_return_shape_and_error(&method.returns);

                AbiCallbackMethod {
                    id: method.id.clone(),
                    vtable_field: naming::vtable_field_name(method.id.as_str()),
                    is_async: method.is_async,
                    params,
                    returns,
                    error,
                }
            })
            .collect();

        AbiCallbackInvocation {
            callback_id: callback.id.clone(),
            vtable_type: naming::callback_vtable_name(callback.id.as_str()),
            register_fn: naming::callback_register_fn(callback.id.as_str()),
            create_fn: naming::callback_create_fn(callback.id.as_str()),
            methods,
        }
    }

    fn abi_stream(&self, class_id: &ClassId, stream: &StreamDef) -> AbiStream {
        let class_name = class_id.as_str();
        let stream_name = stream.id.as_str();
        let item_codec = self.build_codec(&stream.item_type);
        let decode_ops = self.expand_decode(&item_codec);

        AbiStream {
            class_id: class_id.clone(),
            stream_id: stream.id.clone(),
            mode: stream.mode,
            item: StreamItemTransport::WireEncoded { decode_ops },
            subscribe: naming::stream_ffi_subscribe(class_name, stream_name),
            poll: naming::stream_ffi_poll(class_name, stream_name),
            pop_batch: naming::stream_ffi_pop_batch(class_name, stream_name),
            wait: naming::stream_ffi_wait(class_name, stream_name),
            unsubscribe: naming::stream_ffi_unsubscribe(class_name, stream_name),
            free: naming::stream_ffi_free(class_name, stream_name),
        }
    }

    fn abi_record(&self, record: &RecordDef) -> AbiRecord {
        let codec = self.build_codec(&TypeExpr::Record(record.id.clone()));
        let decode_ops = self.expand_decode(&codec);
        let encode_ops = self.expand_encode(&codec, ValueExpr::Instance);
        let (is_blittable, size) = match codec {
            CodecPlan::Record {
                layout: RecordLayout::Blittable { size, .. },
                ..
            } => (true, Some(size)),
            _ => (false, None),
        };

        AbiRecord {
            id: record.id.clone(),
            decode_ops,
            encode_ops,
            is_blittable,
            size,
        }
    }

    fn abi_enum(&self, enumeration: &EnumDef) -> AbiEnum {
        let codec = self.build_codec(&TypeExpr::Enum(enumeration.id.clone()));
        let decode_ops = self.expand_decode(&codec);
        let encode_ops = self.expand_encode(&codec, ValueExpr::Instance);
        let (is_c_style, variants) = match codec {
            CodecPlan::Enum {
                layout: EnumLayout::CStyle { .. },
                ..
            } => (
                true,
                match &enumeration.repr {
                    EnumRepr::CStyle { variants, .. } => variants
                        .iter()
                        .map(|variant| AbiEnumVariant {
                            name: variant.name.clone(),
                            discriminant: variant.discriminant,
                            payload: AbiEnumPayload::Unit,
                        })
                        .collect(),
                    _ => vec![],
                },
            ),
            CodecPlan::Enum {
                layout: EnumLayout::Data { variants, .. },
                ..
            } => (
                false,
                match &enumeration.repr {
                    EnumRepr::Data {
                        variants: data_variants,
                        ..
                    } => {
                        let layout_fields = variants
                            .iter()
                            .map(|variant| {
                                let fields = match &variant.payload {
                                    VariantPayloadLayout::Unit => Vec::new(),
                                    VariantPayloadLayout::Fields(fields) => fields
                                        .iter()
                                        .map(|field| self.abi_enum_field(field))
                                        .collect(),
                                };
                                (variant.name.clone(), fields)
                            })
                            .collect::<HashMap<_, _>>();

                        data_variants
                            .iter()
                            .map(|variant| {
                                let fields = layout_fields
                                    .get(&variant.name)
                                    .cloned()
                                    .unwrap_or_default();
                                let payload = match &variant.payload {
                                    VariantPayload::Unit => AbiEnumPayload::Unit,
                                    VariantPayload::Tuple(_) => AbiEnumPayload::Tuple(fields),
                                    VariantPayload::Struct(_) => AbiEnumPayload::Struct(fields),
                                };
                                AbiEnumVariant {
                                    name: variant.name.clone(),
                                    discriminant: variant.discriminant,
                                    payload,
                                }
                            })
                            .collect()
                    }
                    _ => Vec::new(),
                },
            ),
            _ => (false, vec![]),
        };

        AbiEnum {
            id: enumeration.id.clone(),
            decode_ops,
            encode_ops,
            is_c_style,
            variants,
        }
    }

    fn abi_enum_field(&self, field: &EncodedField) -> AbiEnumField {
        let decode = self.expand_decode(&field.codec);
        let encode = self.expand_encode(
            &field.codec,
            ValueExpr::Named(field.name.as_str().to_string()),
        );
        AbiEnumField {
            name: field.name.clone(),
            type_expr: TypeExpr::from(&field.codec),
            decode,
            encode,
        }
    }

    fn abi_mode_returns_error_for_function(
        &self,
        func: &FunctionDef,
        kind: &CallPlanKind,
    ) -> (CallMode, ReturnShape, ErrorTransport) {
        match kind {
            CallPlanKind::Sync { returns } => {
                let (ret, error) = self.return_shape_and_error(returns);
                (CallMode::Sync, ret, error)
            }
            CallPlanKind::Async { async_plan } => {
                let mode =
                    CallMode::Async(Box::new(self.async_call_for_function(func, async_plan)));
                let ret = ReturnShape {
                    transport: Some(Transport::Scalar(ScalarOrigin::Primitive(
                        PrimitiveType::USize,
                    ))),
                    decode_ops: None,
                    encode_ops: None,
                };
                (mode, ret, ErrorTransport::None)
            }
        }
    }

    fn abi_mode_returns_error_for_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        kind: &CallPlanKind,
    ) -> (CallMode, ReturnShape, ErrorTransport) {
        match kind {
            CallPlanKind::Sync { returns } => {
                let (ret, error) = self.return_shape_and_error(returns);
                (CallMode::Sync, ret, error)
            }
            CallPlanKind::Async { async_plan } => {
                let mode = CallMode::Async(Box::new(
                    self.async_call_for_method(class, method, async_plan),
                ));
                let ret = ReturnShape {
                    transport: Some(Transport::Scalar(ScalarOrigin::Primitive(
                        PrimitiveType::USize,
                    ))),
                    decode_ops: None,
                    encode_ops: None,
                };
                (mode, ret, ErrorTransport::None)
            }
        }
    }

    fn async_call_for_function(&self, func: &FunctionDef, plan: &AsyncPlan) -> AsyncCall {
        let (result, _) = self.return_shape_and_error(&plan.result);
        AsyncCall {
            poll: naming::function_ffi_poll(func.id.as_str()),
            complete: naming::function_ffi_complete(func.id.as_str()),
            cancel: naming::function_ffi_cancel(func.id.as_str()),
            free: naming::function_ffi_free(func.id.as_str()),
            result,
            error: ErrorTransport::StatusCode,
        }
    }

    fn async_call_for_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        plan: &AsyncPlan,
    ) -> AsyncCall {
        let (result, _) = self.return_shape_and_error(&plan.result);
        AsyncCall {
            poll: naming::method_ffi_poll(class.id.as_str(), method.id.as_str()),
            complete: naming::method_ffi_complete(class.id.as_str(), method.id.as_str()),
            cancel: naming::method_ffi_cancel(class.id.as_str(), method.id.as_str()),
            free: naming::method_ffi_free(class.id.as_str(), method.id.as_str()),
            result,
            error: ErrorTransport::StatusCode,
        }
    }

    fn return_shape_and_error(&self, returns: &ReturnPlan) -> (ReturnShape, ErrorTransport) {
        match returns {
            ReturnPlan::Void => (ReturnShape::void(), ErrorTransport::None),
            ReturnPlan::Value(v) => (self.return_shape_from_transport(v), ErrorTransport::None),
            ReturnPlan::Fallible {
                ok: Transport::Handle { class_id, .. },
                err_codec,
            } => (
                ReturnShape {
                    transport: Some(Transport::Handle {
                        class_id: class_id.clone(),
                        nullable: true,
                    }),
                    decode_ops: None,
                    encode_ops: None,
                },
                ErrorTransport::Encoded {
                    decode_ops: self.expand_decode(err_codec),
                    encode_ops: None,
                },
            ),
            ReturnPlan::Fallible { ok, err_codec } => {
                let ok_codec = self.codec_from_transport(ok);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("value".into()));
                let wire_transport = Transport::Span(SpanContent::Encoded(result_codec));
                (
                    return_shape_from_transport_with_ops(wire_transport, decode_ops, encode_ops),
                    ErrorTransport::Encoded {
                        decode_ops: self.expand_decode(err_codec),
                        encode_ops: None,
                    },
                )
            }
        }
    }

    fn return_shape_from_transport(&self, value: &Transport) -> ReturnShape {
        match value {
            Transport::Scalar(origin) => ReturnShape {
                transport: Some(Transport::Scalar(origin.clone())),
                decode_ops: None,
                encode_ops: None,
            },
            Transport::Composite(layout) => {
                let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                let decode_ops = self.expand_decode(&codec);
                let encode_ops = self.expand_encode(&codec, ValueExpr::Var("value".into()));
                ReturnShape {
                    transport: Some(value.clone()),
                    decode_ops: Some(decode_ops),
                    encode_ops: Some(encode_ops),
                }
            }
            Transport::Span(SpanContent::Composite(_) | SpanContent::Scalar(_)) => ReturnShape {
                transport: Some(value.clone()),
                decode_ops: None,
                encode_ops: None,
            },
            Transport::Span(content) => {
                if let Some(composite_transport) = self.try_promote_to_composite_span(content) {
                    return ReturnShape {
                        transport: Some(composite_transport),
                        decode_ops: None,
                        encode_ops: None,
                    };
                }
                let codec = self.codec_from_span_content(content);
                let decode_ops = self.expand_decode(&codec);
                let encode_ops = self.expand_encode(&codec, ValueExpr::Var("value".into()));
                return_shape_from_transport_with_ops(value.clone(), decode_ops, encode_ops)
            }
            transport @ (Transport::Handle { .. } | Transport::Callback { .. }) => ReturnShape {
                transport: Some(transport.clone()),
                decode_ops: None,
                encode_ops: None,
            },
        }
    }

    fn codec_from_transport(&self, value: &Transport) -> CodecPlan {
        match value {
            Transport::Scalar(origin) => CodecPlan::Primitive(origin.primitive()),
            Transport::Composite(layout) => {
                self.build_codec(&TypeExpr::Record(layout.record_id.clone()))
            }
            Transport::Span(content) => self.codec_from_span_content(content),
            Transport::Handle { .. } | Transport::Callback { .. } => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    fn try_promote_to_composite_span(&self, content: &SpanContent) -> Option<Transport> {
        let SpanContent::Encoded(CodecPlan::Vec { element, .. }) = content else {
            return None;
        };
        let CodecPlan::Record { id, .. } = element.as_ref() else {
            return None;
        };
        match self.classify_record(id) {
            Transport::Composite(layout) => Some(Transport::Span(SpanContent::Composite(layout))),
            _ => None,
        }
    }

    fn codec_from_span_content(&self, content: &SpanContent) -> CodecPlan {
        match content {
            SpanContent::Scalar(origin) => {
                let p = origin.primitive();
                CodecPlan::Vec {
                    element: Box::new(CodecPlan::Primitive(p)),
                    layout: VecLayout::Blittable {
                        element_size: p.wire_size_bytes(),
                    },
                }
            }
            SpanContent::Composite(layout) => {
                let element_codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                CodecPlan::Vec {
                    element: Box::new(element_codec),
                    layout: VecLayout::Blittable {
                        element_size: layout.total_size,
                    },
                }
            }
            SpanContent::Utf8 => CodecPlan::String,
            SpanContent::Encoded(codec) => codec.clone(),
        }
    }

    fn primitive_from_abi(&self, abi: AbiType) -> PrimitiveType {
        match abi {
            AbiType::Bool => PrimitiveType::Bool,
            AbiType::I8 => PrimitiveType::I8,
            AbiType::U8 => PrimitiveType::U8,
            AbiType::I16 => PrimitiveType::I16,
            AbiType::U16 => PrimitiveType::U16,
            AbiType::I32 => PrimitiveType::I32,
            AbiType::U32 => PrimitiveType::U32,
            AbiType::I64 => PrimitiveType::I64,
            AbiType::U64 => PrimitiveType::U64,
            AbiType::ISize => PrimitiveType::ISize,
            AbiType::USize => PrimitiveType::USize,
            AbiType::F32 => PrimitiveType::F32,
            AbiType::F64 => PrimitiveType::F64,
            AbiType::Void
            | AbiType::Pointer(_)
            | AbiType::InlineCallbackFn { .. }
            | AbiType::Handle(_)
            | AbiType::CallbackHandle
            | AbiType::Struct(_) => {
                panic!("unsupported ABI primitive for wire encoding")
            }
        }
    }

    fn expand_decode(&self, codec: &CodecPlan) -> ReadSeq {
        self.expand_decode_with_offset(codec, "pos")
    }

    // self is only used in recursive calls, records, enums, vecs all recurse back here,
    // but clippy does not see through the recursion and thinks it is unused.
    #[allow(clippy::only_used_in_recursion)]
    fn expand_decode_with_offset(&self, codec: &CodecPlan, base: &str) -> ReadSeq {
        let offset = OffsetExpr::Base;
        match codec {
            CodecPlan::Void => ReadSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            },
            CodecPlan::Primitive(primitive) => ReadSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![ReadOp::Primitive {
                    primitive: *primitive,
                    offset,
                }],
                shape: WireShape::Value,
            },
            CodecPlan::String => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::String { offset }],
                shape: WireShape::Value,
            },
            CodecPlan::Bytes => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Bytes { offset }],
                shape: WireShape::Value,
            },
            CodecPlan::Builtin(id) => {
                let size = self
                    .builtin_fixed_size(id)
                    .map(SizeExpr::Fixed)
                    .unwrap_or(SizeExpr::Runtime);
                ReadSeq {
                    size,
                    ops: vec![ReadOp::Builtin {
                        id: id.clone(),
                        offset,
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Option(inner) => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Option {
                    tag_offset: offset,
                    some: Box::new(self.expand_decode_with_offset(inner, "pos")),
                }],
                shape: WireShape::Optional,
            },
            CodecPlan::Vec { element, layout } => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Vec {
                    len_offset: offset,
                    element_type: TypeExpr::from(element.as_ref()),
                    element: Box::new(self.expand_decode_with_offset(element, "pos")),
                    layout: layout.clone(),
                }],
                shape: WireShape::Sequence,
            },
            CodecPlan::Result { ok, err } => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Result {
                    tag_offset: offset,
                    ok: Box::new(self.expand_decode_with_offset(ok, "pos")),
                    err: Box::new(self.expand_decode_with_offset(err, "pos")),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Record { id, layout } => {
                let (fields, size) = match layout {
                    RecordLayout::Blittable { fields, size } => (
                        fields
                            .iter()
                            .map(|field| {
                                let offset_expr = if field.offset == 0 {
                                    OffsetExpr::Base
                                } else {
                                    OffsetExpr::BasePlus(field.offset)
                                };
                                FieldReadOp {
                                    name: field.name.clone(),
                                    seq: ReadSeq {
                                        size: SizeExpr::Fixed(field.primitive.wire_size_bytes()),
                                        ops: vec![ReadOp::Primitive {
                                            primitive: field.primitive,
                                            offset: offset_expr,
                                        }],
                                        shape: WireShape::Value,
                                    },
                                }
                            })
                            .collect(),
                        SizeExpr::Fixed(*size),
                    ),
                    RecordLayout::Encoded { fields } => (
                        fields
                            .iter()
                            .map(|field| FieldReadOp {
                                name: field.name.clone(),
                                seq: self.expand_decode_with_offset(&field.codec, "pos"),
                            })
                            .collect(),
                        SizeExpr::Runtime,
                    ),
                    RecordLayout::Recursive => (vec![], SizeExpr::Runtime),
                };
                ReadSeq {
                    size,
                    ops: vec![ReadOp::Record {
                        id: id.clone(),
                        offset,
                        fields,
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Enum { id, layout } => ReadSeq {
                size: match layout {
                    EnumLayout::CStyle { .. } => SizeExpr::Fixed(4),
                    EnumLayout::Data { .. } | EnumLayout::Recursive => SizeExpr::Runtime,
                },
                ops: vec![ReadOp::Enum {
                    id: id.clone(),
                    offset,
                    layout: layout.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Custom { id, underlying } => {
                let underlying_seq = self.expand_decode_with_offset(underlying, base);
                ReadSeq {
                    size: underlying_seq.size.clone(),
                    ops: vec![ReadOp::Custom {
                        id: id.clone(),
                        underlying: Box::new(underlying_seq),
                    }],
                    shape: WireShape::Value,
                }
            }
        }
    }

    fn expand_encode(&self, codec: &CodecPlan, value: ValueExpr) -> WriteSeq {
        match codec {
            CodecPlan::Void => WriteSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            },
            CodecPlan::Primitive(primitive) => WriteSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![WriteOp::Primitive {
                    primitive: *primitive,
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::String => WriteSeq {
                size: SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::StringLen(value.clone())]),
                ops: vec![WriteOp::String {
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Bytes => WriteSeq {
                size: SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::BytesLen(value.clone())]),
                ops: vec![WriteOp::Bytes {
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Builtin(id) => WriteSeq {
                size: self
                    .builtin_fixed_size(id)
                    .map(SizeExpr::Fixed)
                    .unwrap_or_else(|| {
                        if id.as_str() == "Url" {
                            SizeExpr::Sum(vec![
                                SizeExpr::Fixed(4),
                                SizeExpr::BuiltinSize {
                                    id: id.clone(),
                                    value: value.clone(),
                                },
                            ])
                        } else {
                            SizeExpr::WireSize {
                                value: value.clone(),
                                record_id: None,
                            }
                        }
                    }),
                ops: vec![WriteOp::Builtin {
                    id: id.clone(),
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Option(inner) => {
                let inner_seq = self.expand_encode(inner, ValueExpr::Var("v".into()));
                WriteSeq {
                    size: SizeExpr::OptionSize {
                        value: value.clone(),
                        inner: Box::new(inner_seq.size.clone()),
                    },
                    ops: vec![WriteOp::Option {
                        value: value.clone(),
                        some: Box::new(inner_seq),
                    }],
                    shape: WireShape::Optional,
                }
            }
            CodecPlan::Vec { element, layout } => {
                let element_seq = self.expand_encode(element, ValueExpr::Var("item".into()));
                let size_expr =
                    if matches!(element.as_ref(), CodecPlan::Primitive(PrimitiveType::U8)) {
                        SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::BytesLen(value.clone())])
                    } else {
                        SizeExpr::VecSize {
                            value: value.clone(),
                            inner: Box::new(element_seq.size.clone()),
                            layout: layout.clone(),
                        }
                    };
                WriteSeq {
                    size: size_expr,
                    ops: vec![WriteOp::Vec {
                        value: value.clone(),
                        element_type: TypeExpr::from(element.as_ref()),
                        element: Box::new(element_seq),
                        layout: layout.clone(),
                    }],
                    shape: WireShape::Sequence,
                }
            }
            CodecPlan::Result { ok, err } => {
                let ok_seq = self.expand_encode(ok, ValueExpr::Var("okVal".into()));
                let err_seq = self.expand_encode(err, ValueExpr::Var("errVal".into()));
                WriteSeq {
                    size: SizeExpr::ResultSize {
                        value: value.clone(),
                        ok: Box::new(ok_seq.size.clone()),
                        err: Box::new(err_seq.size.clone()),
                    },
                    ops: vec![WriteOp::Result {
                        value: value.clone(),
                        ok: Box::new(ok_seq),
                        err: Box::new(err_seq),
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Record { id, layout } => {
                let fields = match layout {
                    RecordLayout::Blittable { fields, .. } => fields
                        .iter()
                        .map(|field| {
                            let field_value = value.field(field.name.clone());
                            FieldWriteOp {
                                name: field.name.clone(),
                                accessor: field_value.clone(),
                                seq: self.expand_encode(
                                    &CodecPlan::Primitive(field.primitive),
                                    field_value,
                                ),
                            }
                        })
                        .collect(),
                    RecordLayout::Encoded { fields } => fields
                        .iter()
                        .map(|field| {
                            let field_value = value.field(field.name.clone());
                            FieldWriteOp {
                                name: field.name.clone(),
                                accessor: field_value.clone(),
                                seq: self.expand_encode(&field.codec, field_value),
                            }
                        })
                        .collect(),
                    RecordLayout::Recursive => vec![],
                };
                let size = match layout {
                    RecordLayout::Blittable { size, .. } => SizeExpr::Fixed(*size),
                    _ => SizeExpr::WireSize {
                        value: value.clone(),
                        record_id: Some(id.clone()),
                    },
                };
                WriteSeq {
                    size,
                    ops: vec![WriteOp::Record {
                        id: id.clone(),
                        value: value.clone(),
                        fields,
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Enum { id, layout } => {
                let size = match layout {
                    EnumLayout::CStyle { .. } => SizeExpr::Fixed(4),
                    _ => SizeExpr::WireSize {
                        value: value.clone(),
                        record_id: None,
                    },
                };
                WriteSeq {
                    size,
                    ops: vec![WriteOp::Enum {
                        id: id.clone(),
                        value: value.clone(),
                        layout: layout.clone(),
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Custom { id, underlying } => {
                let underlying_seq = self.expand_encode(underlying, value.clone());
                WriteSeq {
                    size: underlying_seq.size.clone(),
                    ops: vec![WriteOp::Custom {
                        id: id.clone(),
                        value: value.clone(),
                        underlying: Box::new(underlying_seq),
                    }],
                    shape: WireShape::Value,
                }
            }
        }
    }

    fn builtin_fixed_size(&self, id: &BuiltinId) -> Option<usize> {
        match id.as_str() {
            "Duration" | "SystemTime" => Some(12),
            "Uuid" => Some(16),
            _ => None,
        }
    }

    fn abi_params_from_plan(&self, params: &[ParamPlan]) -> Vec<AbiParam> {
        params
            .iter()
            .flat_map(|param| self.abi_param_from_plan(param))
            .collect()
    }

    fn abi_param_from_plan(&self, param: &ParamPlan) -> Vec<AbiParam> {
        let len_name = ParamName::new(format!("{}_len", param.name.as_str()));

        let make_span_params = |transport: Transport,
                                mutability: Mutability,
                                decode_ops: Option<ReadSeq>,
                                encode_ops: Option<WriteSeq>|
         -> Vec<AbiParam> {
            let ptr_element = match &transport {
                Transport::Span(SpanContent::Scalar(origin)) => origin.primitive(),
                _ => PrimitiveType::U8,
            };
            vec![
                AbiParam {
                    name: param.name.clone(),
                    abi_type: AbiType::Pointer(ptr_element),
                    role: ParamRole::Input {
                        transport,
                        mutability,
                        len_param: Some(len_name.clone()),
                        decode_ops,
                        encode_ops,
                    },
                },
                AbiParam {
                    name: len_name.clone(),
                    abi_type: AbiType::USize,
                    role: ParamRole::SyntheticLen {
                        for_param: param.name.clone(),
                    },
                },
            ]
        };

        match &param.transport {
            Transport::Scalar(origin) => vec![AbiParam {
                name: param.name.clone(),
                abi_type: AbiType::from(origin.primitive()),
                role: ParamRole::Input {
                    transport: param.transport.clone(),
                    mutability: param.mutability,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            Transport::Composite(layout) => {
                let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                let decode_ops = self.expand_decode(&codec);
                let encode_ops =
                    self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                vec![AbiParam {
                    name: param.name.clone(),
                    abi_type: AbiType::Struct(layout.record_id.clone()),
                    role: ParamRole::Input {
                        transport: Transport::Composite(layout.clone()),
                        mutability: param.mutability,
                        len_param: None,
                        decode_ops: Some(decode_ops),
                        encode_ops: Some(encode_ops),
                    },
                }]
            }
            span @ Transport::Span(content) => match content {
                SpanContent::Scalar(_) | SpanContent::Utf8 => {
                    make_span_params(span.clone(), param.mutability, None, None)
                }
                SpanContent::Composite(layout) => {
                    let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                    let decode_ops = self.expand_decode(&codec);
                    let encode_ops = self
                        .expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                    make_span_params(
                        span.clone(),
                        param.mutability,
                        Some(decode_ops),
                        Some(encode_ops),
                    )
                }
                SpanContent::Encoded(codec) => {
                    let decode_ops = self.expand_decode(codec);
                    let encode_ops = self
                        .expand_encode(codec, ValueExpr::Named(param.name.as_str().to_string()));
                    make_span_params(
                        span.clone(),
                        param.mutability,
                        Some(decode_ops),
                        Some(encode_ops),
                    )
                }
            },
            Transport::Handle { class_id, .. } => vec![AbiParam {
                name: param.name.clone(),
                abi_type: AbiType::Handle(class_id.clone()),
                role: ParamRole::Input {
                    transport: param.transport.clone(),
                    mutability: param.mutability,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            Transport::Callback {
                style: CallbackStyle::BoxedDyn,
                ..
            } => vec![AbiParam {
                name: param.name.clone(),
                abi_type: AbiType::CallbackHandle,
                role: ParamRole::Input {
                    transport: param.transport.clone(),
                    mutability: param.mutability,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            Transport::Callback {
                style: CallbackStyle::ImplTrait,
                callback_id,
                ..
            } => {
                let ud_name = ParamName::new(format!("{}_ud", param.name.as_str()));
                let (fn_params, fn_return_type) =
                    self.inline_callback_fn_abi_signature(callback_id);
                vec![
                    AbiParam {
                        name: param.name.clone(),
                        abi_type: AbiType::InlineCallbackFn {
                            params: fn_params,
                            return_type: Box::new(fn_return_type),
                        },
                        role: ParamRole::Input {
                            transport: param.transport.clone(),
                            mutability: param.mutability,
                            len_param: Some(ud_name.clone()),
                            decode_ops: None,
                            encode_ops: None,
                        },
                    },
                    AbiParam {
                        name: ud_name,
                        abi_type: AbiType::Pointer(PrimitiveType::U8),
                        role: ParamRole::CallbackContext {
                            for_param: param.name.clone(),
                        },
                    },
                ]
            }
        }
    }

    fn inline_callback_fn_abi_signature(
        &self,
        callback_id: &CallbackId,
    ) -> (Vec<AbiType>, AbiType) {
        let callback_def = self
            .contract
            .catalog
            .resolve_callback(callback_id)
            .unwrap_or_else(|| panic!("callback {} not found", callback_id.as_str()));

        let mut abi_params = vec![];

        for method in &callback_def.methods {
            for param in &method.params {
                let transport = self.classify_type(&param.type_expr);
                match &transport {
                    Transport::Scalar(origin) => {
                        abi_params.push(AbiType::from(origin.primitive()));
                    }
                    _ => {
                        abi_params.push(AbiType::Pointer(PrimitiveType::U8));
                        abi_params.push(AbiType::USize);
                    }
                }
            }
        }

        let return_type = callback_def
            .methods
            .first()
            .and_then(|method| match &method.returns {
                ReturnDef::Void => None,
                ReturnDef::Value(ty) => {
                    let transport = self.classify_type(ty);
                    match &transport {
                        Transport::Scalar(origin) => Some(AbiType::from(origin.primitive())),
                        Transport::Composite(layout) => {
                            Some(AbiType::Struct(layout.record_id.clone()))
                        }
                        _ => Some(AbiType::Pointer(PrimitiveType::U8)),
                    }
                }
                ReturnDef::Result { .. } => Some(AbiType::Pointer(PrimitiveType::U8)),
            })
            .unwrap_or(AbiType::Void);

        (abi_params, return_type)
    }

    fn callback_return_shape_and_error(
        &self,
        returns: &ReturnDef,
    ) -> (ReturnShape, ErrorTransport) {
        match returns {
            ReturnDef::Void => (ReturnShape::void(), ErrorTransport::None),
            ReturnDef::Value(ty) => {
                let transport = self.classify_type(ty);
                let shape = match &transport {
                    Transport::Scalar(_) => self.return_shape_from_transport(&transport),
                    _ => {
                        let codec = self.build_codec(ty);
                        let decode_ops = self.expand_decode(&codec);
                        let encode_ops = self.expand_encode(&codec, ValueExpr::Var("value".into()));
                        let wire_transport = Transport::Span(SpanContent::Encoded(codec));
                        return_shape_from_transport_with_ops(wire_transport, decode_ops, encode_ops)
                    }
                };
                (shape, ErrorTransport::None)
            }
            ReturnDef::Result { ok, err } => {
                let ok_codec = self.build_codec(ok);
                let err_codec = self.build_codec(err);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("result".into()));
                let wire_transport = Transport::Span(SpanContent::Encoded(result_codec));
                (
                    return_shape_from_transport_with_ops(wire_transport, decode_ops, encode_ops),
                    ErrorTransport::Encoded {
                        decode_ops: self.expand_decode(&err_codec),
                        encode_ops: Some(
                            self.expand_encode(&err_codec, ValueExpr::Var("error".into())),
                        ),
                    },
                )
            }
        }
    }

    fn abi_callback_params<'a>(
        &'a self,
        callback: &'a CallbackTraitDef,
        method: &'a CallbackMethodDef,
    ) -> impl Iterator<Item = AbiParam> + 'a {
        let handle_param = AbiParam {
            name: ParamName::new("handle"),
            abi_type: AbiType::Pointer(PrimitiveType::U8),
            role: ParamRole::Input {
                transport: Transport::Callback {
                    callback_id: callback.id.clone(),
                    nullable: false,
                    style: CallbackStyle::BoxedDyn,
                },
                mutability: Mutability::Shared,
                len_param: None,
                decode_ops: None,
                encode_ops: None,
            },
        };

        let method_params = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(param))
            .flat_map(|param| self.abi_callback_param_from_plan(param));

        let out_params = self.abi_callback_out_params(&method.returns, method.is_async);

        std::iter::once(handle_param)
            .chain(method_params)
            .chain(out_params)
    }

    fn abi_callback_param_from_plan(&self, param: AbiCallbackParamPlan) -> Vec<AbiParam> {
        let len_name = ParamName::new(format!("{}_len", param.name.as_str()));

        match param.strategy {
            AbiCallbackParamStrategy::Scalar(p) => vec![AbiParam {
                name: param.name,
                abi_type: AbiType::from(p),
                role: ParamRole::Input {
                    transport: Transport::Scalar(ScalarOrigin::Primitive(p)),
                    mutability: Mutability::Shared,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            AbiCallbackParamStrategy::Direct(layout) => {
                let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                let decode_ops = self.expand_decode(&codec);
                let encode_ops =
                    self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                vec![AbiParam {
                    name: param.name.clone(),
                    abi_type: AbiType::Struct(layout.record_id.clone()),
                    role: ParamRole::Input {
                        transport: Transport::Composite(layout),
                        mutability: Mutability::Shared,
                        len_param: None,
                        decode_ops: Some(decode_ops),
                        encode_ops: Some(encode_ops),
                    },
                }]
            }
            AbiCallbackParamStrategy::Encoded { codec } => {
                let decode_ops = self.expand_decode(&codec);
                let encode_ops =
                    self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                vec![
                    AbiParam {
                        name: param.name.clone(),
                        abi_type: AbiType::Pointer(PrimitiveType::U8),
                        role: ParamRole::Input {
                            transport: Transport::Span(SpanContent::Encoded(codec)),
                            mutability: Mutability::Shared,
                            len_param: Some(len_name.clone()),
                            decode_ops: Some(decode_ops),
                            encode_ops: Some(encode_ops),
                        },
                    },
                    AbiParam {
                        name: len_name,
                        abi_type: AbiType::USize,
                        role: ParamRole::SyntheticLen {
                            for_param: param.name,
                        },
                    },
                ]
            }
        }
    }

    fn abi_callback_out_params(&self, returns: &ReturnDef, is_async: bool) -> Vec<AbiParam> {
        let has_return = !matches!(returns, ReturnDef::Void) && !is_async;
        let out_ptr_name = ParamName::new("out_ptr");
        let out_len_name = ParamName::new("out_len");

        if !has_return {
            return Vec::new();
        }

        let (ret, _) = self.callback_return_shape_and_error(returns);

        match &ret.transport {
            Some(Transport::Scalar(origin)) => vec![AbiParam {
                name: out_ptr_name,
                abi_type: AbiType::from(origin.primitive()),
                role: ParamRole::OutDirect,
            }],
            Some(Transport::Handle { .. } | Transport::Callback { .. }) | None => {
                vec![AbiParam {
                    name: out_ptr_name,
                    abi_type: AbiType::Pointer(PrimitiveType::U8),
                    role: ParamRole::OutDirect,
                }]
            }
            Some(_) => {
                vec![
                    AbiParam {
                        name: out_ptr_name.clone(),
                        abi_type: AbiType::Pointer(PrimitiveType::U8),
                        role: ParamRole::OutDirect,
                    },
                    AbiParam {
                        name: out_len_name,
                        abi_type: AbiType::USize,
                        role: ParamRole::OutLen {
                            for_param: out_ptr_name,
                        },
                    },
                ]
            }
        }
    }
}

impl<'c> Lowerer<'c> {
    fn lower_function(&self, func: &FunctionDef) -> CallPlan {
        let params = func.params.iter().map(|p| self.lower_param(p)).collect();

        let kind = if func.is_async {
            CallPlanKind::Async {
                async_plan: self.build_async_plan(&func.returns),
            }
        } else {
            CallPlanKind::Sync {
                returns: self.lower_return(&func.returns),
            }
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.function_symbol(&func.id)),
            params,
            kind,
        }
    }

    fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> CallPlan {
        let mut params: Vec<ParamPlan> =
            method.params.iter().map(|p| self.lower_param(p)).collect();

        if method.receiver != Receiver::Static {
            params.insert(
                0,
                ParamPlan {
                    name: ParamName::new("self"),
                    transport: Transport::Handle {
                        class_id: class.id.clone(),
                        nullable: false,
                    },
                    mutability: Mutability::Shared,
                },
            );
        }

        let kind = if method.is_async {
            CallPlanKind::Async {
                async_plan: self.build_async_plan(&method.returns),
            }
        } else {
            CallPlanKind::Sync {
                returns: self.lower_return(&method.returns),
            }
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.method_symbol(&class.id, &method.id)),
            params,
            kind,
        }
    }

    fn lower_constructor(&self, class: &ClassDef, ctor: &ConstructorDef) -> CallPlan {
        let params = ctor
            .params()
            .into_iter()
            .map(|p| self.lower_param(p))
            .collect();

        let returns = if ctor.is_fallible() {
            ReturnPlan::Fallible {
                ok: Transport::Handle {
                    class_id: class.id.clone(),
                    nullable: false,
                },
                err_codec: CodecPlan::String,
            }
        } else {
            ReturnPlan::Value(Transport::Handle {
                class_id: class.id.clone(),
                nullable: false,
            })
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.constructor_symbol(&class.id, ctor.name())),
            params,
            kind: CallPlanKind::Sync { returns },
        }
    }

    fn lower_callback(&self, callback: &CallbackTraitDef) -> Vec<CallPlan> {
        callback
            .methods
            .iter()
            .map(|method| {
                let mut params: Vec<ParamPlan> =
                    method.params.iter().map(|p| self.lower_param(p)).collect();

                params.insert(
                    0,
                    ParamPlan {
                        name: ParamName::new("callback"),
                        transport: Transport::Callback {
                            callback_id: callback.id.clone(),
                            style: CallbackStyle::BoxedDyn,
                            nullable: false,
                        },
                        mutability: Mutability::Shared,
                    },
                );

                let kind = if method.is_async {
                    CallPlanKind::Async {
                        async_plan: self.build_async_plan(&method.returns),
                    }
                } else {
                    CallPlanKind::Sync {
                        returns: self.lower_return(&method.returns),
                    }
                };

                CallPlan {
                    target: CallTarget::VtableField(naming::vtable_field_name(method.id.as_str())),
                    params,
                    kind,
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Param/Return Lowering
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    fn lower_param(&self, param: &ParamDef) -> ParamPlan {
        let mutability = match param.passing {
            ParamPassing::RefMut => Mutability::Mutable,
            _ => Mutability::Shared,
        };
        ParamPlan {
            name: param.name.clone(),
            transport: self.classify_param(&param.type_expr, &param.passing),
            mutability,
        }
    }

    fn classify_param(&self, type_expr: &TypeExpr, passing: &ParamPassing) -> Transport {
        if let (ParamPassing::ImplTrait | ParamPassing::BoxedDyn, TypeExpr::Callback(id)) =
            (passing, type_expr)
        {
            let style = match passing {
                ParamPassing::ImplTrait => CallbackStyle::ImplTrait,
                ParamPassing::BoxedDyn => CallbackStyle::BoxedDyn,
                _ => unreachable!(),
            };
            return Transport::Callback {
                callback_id: id.clone(),
                style,
                nullable: false,
            };
        }

        self.classify_type(type_expr)
    }

    fn classify_type(&self, type_expr: &TypeExpr) -> Transport {
        match type_expr {
            TypeExpr::Primitive(p) => Transport::Scalar(ScalarOrigin::Primitive(*p)),

            TypeExpr::Enum(id) => self.classify_enum(id),

            TypeExpr::String => Transport::Span(SpanContent::Utf8),

            TypeExpr::Bytes => Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::U8,
            ))),

            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => {
                    Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(*p)))
                }
                TypeExpr::Enum(id) => match self.classify_enum(id) {
                    Transport::Scalar(origin) => Transport::Span(SpanContent::Scalar(origin)),
                    _ => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
                },
                _ => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
            },

            TypeExpr::Handle(class_id) => Transport::Handle {
                class_id: class_id.clone(),
                nullable: false,
            },

            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(class_id) => Transport::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                TypeExpr::Callback(callback_id) => Transport::Callback {
                    callback_id: callback_id.clone(),
                    style: CallbackStyle::BoxedDyn,
                    nullable: true,
                },
                _ => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
            },

            TypeExpr::Callback(callback_id) => Transport::Callback {
                callback_id: callback_id.clone(),
                style: CallbackStyle::BoxedDyn,
                nullable: false,
            },

            TypeExpr::Record(id) => self.classify_record(id),

            TypeExpr::Result { .. } | TypeExpr::Custom(_) | TypeExpr::Builtin(_) => {
                Transport::Span(SpanContent::Encoded(self.build_codec(type_expr)))
            }

            TypeExpr::Void => Transport::Scalar(ScalarOrigin::Primitive(PrimitiveType::U8)),
        }
    }

    fn classify_enum(&self, id: &EnumId) -> Transport {
        let def = self
            .contract
            .catalog
            .resolve_enum(id)
            .unwrap_or_else(|| panic!("unresolved enum: {}", id.as_str()));

        match &def.repr {
            EnumRepr::CStyle { tag_type, .. } => Transport::Scalar(ScalarOrigin::CStyleEnum {
                tag_type: *tag_type,
                enum_id: id.clone(),
            }),
            EnumRepr::Data { .. } => Transport::Span(SpanContent::Encoded(
                self.build_codec(&TypeExpr::Enum(id.clone())),
            )),
        }
    }

    fn classify_record(&self, id: &RecordId) -> Transport {
        let def = self
            .contract
            .catalog
            .resolve_record(id)
            .unwrap_or_else(|| panic!("unresolved record: {}", id.as_str()));

        if self.is_blittable_record(def) {
            let (total_size, blittable_fields) = compute_blittable_layout(def);
            let fields = blittable_fields
                .into_iter()
                .map(|bf| CompositeField {
                    name: bf.name,
                    offset: bf.offset,
                    primitive: bf.primitive,
                })
                .collect();
            Transport::Composite(CompositeLayout {
                record_id: id.clone(),
                total_size,
                fields,
            })
        } else {
            Transport::Span(SpanContent::Encoded(
                self.build_codec(&TypeExpr::Record(id.clone())),
            ))
        }
    }

    fn lower_return(&self, returns: &ReturnDef) -> ReturnPlan {
        match returns {
            ReturnDef::Void => ReturnPlan::Void,
            ReturnDef::Value(ty) => ReturnPlan::Value(self.classify_type(ty)),
            ReturnDef::Result { ok, err } => ReturnPlan::Fallible {
                ok: self.classify_type(ok),
                err_codec: self.build_codec(err),
            },
        }
    }

    fn build_async_plan(&self, returns: &ReturnDef) -> AsyncPlan {
        AsyncPlan {
            completion_callback: CompletionCallback {
                param_name: ParamName::new("completion"),
                abi_type: AbiType::Pointer(PrimitiveType::U8),
            },
            result: self.lower_return(returns),
        }
    }
}

impl<'c> Lowerer<'c> {
    fn lower_callback_param(&self, param: &ParamDef) -> AbiCallbackParamPlan {
        let strategy = match &param.type_expr {
            TypeExpr::Primitive(p) => AbiCallbackParamStrategy::Scalar(*p),
            _ => AbiCallbackParamStrategy::Encoded {
                codec: self.build_codec(&param.type_expr),
            },
        };

        AbiCallbackParamPlan {
            name: param.name.clone(),
            strategy,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Codec Building
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn build_codec(&self, type_expr: &TypeExpr) -> CodecPlan {
        match type_expr {
            TypeExpr::Void => CodecPlan::Void,
            TypeExpr::Primitive(p) => CodecPlan::Primitive(*p),
            TypeExpr::String => CodecPlan::String,
            TypeExpr::Bytes => CodecPlan::Bytes,
            TypeExpr::Builtin(id) => CodecPlan::Builtin(id.clone()),

            TypeExpr::Option(inner) => CodecPlan::Option(Box::new(self.build_codec(inner))),

            TypeExpr::Vec(inner) => CodecPlan::Vec {
                element: Box::new(self.build_codec(inner)),
                layout: self.vec_layout(inner),
            },

            TypeExpr::Result { ok, err } => CodecPlan::Result {
                ok: Box::new(self.build_codec(ok)),
                err: Box::new(self.build_codec(err)),
            },

            TypeExpr::Record(id) => CodecPlan::Record {
                id: id.clone(),
                layout: self.record_layout(id),
            },

            TypeExpr::Enum(id) => CodecPlan::Enum {
                id: id.clone(),
                layout: self.enum_layout(id),
            },

            TypeExpr::Custom(id) => {
                let def = self
                    .contract
                    .catalog
                    .resolve_custom(id)
                    .expect("custom type should be resolved");
                CodecPlan::Custom {
                    id: id.clone(),
                    underlying: Box::new(self.build_codec(&def.repr)),
                }
            }

            TypeExpr::Handle(_) | TypeExpr::Callback(_) => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    fn record_layout(&self, id: &RecordId) -> RecordLayout {
        if self.record_stack.borrow().contains(id) {
            return RecordLayout::Recursive;
        }

        self.record_stack.borrow_mut().insert(id.clone());

        let def = self
            .contract
            .catalog
            .resolve_record(id)
            .expect("record should be resolved");

        let layout = if self.is_blittable_record(def) {
            self.build_blittable_record_layout(def)
        } else {
            self.build_encoded_record_layout(def)
        };

        self.record_stack.borrow_mut().remove(id);
        layout
    }

    fn is_blittable_record(&self, def: &RecordDef) -> bool {
        let field_primitives: Vec<_> = def
            .fields
            .iter()
            .filter_map(|f| match &f.type_expr {
                TypeExpr::Primitive(p) => Some(p.to_field_primitive()),
                _ => None,
            })
            .collect();
        let all_primitive = field_primitives.len() == def.fields.len();
        let classify_fields = if all_primitive {
            &field_primitives[..]
        } else {
            &[]
        };
        matches!(
            classification::classify_struct(def.is_repr_c, classify_fields),
            PassableCategory::Blittable,
        )
    }

    fn build_blittable_record_layout(&self, def: &RecordDef) -> RecordLayout {
        let (size, fields) = compute_blittable_layout(def);
        RecordLayout::Blittable { size, fields }
    }

    fn build_encoded_record_layout(&self, def: &RecordDef) -> RecordLayout {
        let fields = def
            .fields
            .iter()
            .map(|f| EncodedField {
                name: f.name.clone(),
                codec: self.build_codec(&f.type_expr),
            })
            .collect();

        RecordLayout::Encoded { fields }
    }

    fn enum_layout(&self, id: &EnumId) -> EnumLayout {
        if self.enum_stack.borrow().contains(id) {
            return EnumLayout::Recursive;
        }

        self.enum_stack.borrow_mut().insert(id.clone());

        let def = self
            .contract
            .catalog
            .resolve_enum(id)
            .expect("enum should be resolved");

        let layout = match &def.repr {
            EnumRepr::CStyle { tag_type, .. } => EnumLayout::CStyle {
                tag_type: *tag_type,
                is_error: def.is_error,
            },

            EnumRepr::Data { tag_type, variants } => EnumLayout::Data {
                tag_type: *tag_type,
                variants: variants
                    .iter()
                    .map(|v| VariantLayout {
                        name: v.name.clone(),
                        discriminant: v.discriminant,
                        payload: self.variant_payload_layout(&v.payload),
                    })
                    .collect(),
            },
        };

        self.enum_stack.borrow_mut().remove(id);
        layout
    }

    fn variant_payload_layout(&self, payload: &VariantPayload) -> VariantPayloadLayout {
        match payload {
            VariantPayload::Unit => VariantPayloadLayout::Unit,
            VariantPayload::Tuple(types) => VariantPayloadLayout::Fields(
                types
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| EncodedField {
                        name: FieldName::new(format!("value_{}", idx)),
                        codec: self.build_codec(ty),
                    })
                    .collect(),
            ),
            VariantPayload::Struct(fields) => VariantPayloadLayout::Fields(
                fields
                    .iter()
                    .map(|f| EncodedField {
                        name: f.name.clone(),
                        codec: self.build_codec(&f.type_expr),
                    })
                    .collect(),
            ),
        }
    }

    fn vec_layout(&self, element: &TypeExpr) -> VecLayout {
        match element {
            TypeExpr::Primitive(p) => match p.size_bytes() {
                Some(size) => VecLayout::Blittable { element_size: size },
                None => VecLayout::Encoded,
            },

            TypeExpr::Record(id) => {
                let def = self.contract.catalog.resolve_record(id);
                match def {
                    Some(def) if self.is_blittable_record(def) => VecLayout::Blittable {
                        element_size: self.blittable_record_size(def),
                    },
                    _ => VecLayout::Encoded,
                }
            }

            _ => VecLayout::Encoded,
        }
    }

    fn blittable_record_size(&self, def: &RecordDef) -> usize {
        let (size, _) = compute_blittable_layout(def);
        size
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Symbol Generation
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    fn function_symbol(&self, id: &FunctionId) -> naming::Name<naming::GlobalSymbol> {
        naming::function_ffi_name(id.as_str())
    }

    fn method_symbol(
        &self,
        class_id: &ClassId,
        method_id: &MethodId,
    ) -> naming::Name<naming::GlobalSymbol> {
        naming::method_ffi_name(class_id.as_str(), method_id.as_str())
    }

    fn constructor_symbol(
        &self,
        class_id: &ClassId,
        name: Option<&MethodId>,
    ) -> naming::Name<naming::GlobalSymbol> {
        match name {
            Some(n) => naming::method_ffi_name(class_id.as_str(), n.as_str()),
            None => naming::class_ffi_new(class_id.as_str()),
        }
    }

    fn call_symbol(&self, plan: &CallPlan) -> naming::Name<naming::GlobalSymbol> {
        match &plan.target {
            CallTarget::GlobalSymbol(symbol) => symbol.clone(),
            CallTarget::VtableField(_) => panic!("expected global symbol"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn align_up(offset: usize, alignment: usize) -> usize {
    (offset + alignment - 1) & !(alignment - 1)
}

fn compute_blittable_layout(def: &RecordDef) -> (usize, Vec<BlittableField>) {
    let (final_offset, fields) =
        def.fields
            .iter()
            .fold((0usize, Vec::new()), |(offset, mut fields), field| {
                let TypeExpr::Primitive(p) = &field.type_expr else {
                    panic!("blittable record should only have primitive fields");
                };

                let alignment = p
                    .alignment()
                    .expect("blittable field must have fixed-size alignment");
                let size = p
                    .size_bytes()
                    .expect("blittable field must have fixed size");
                let aligned_offset = align_up(offset, alignment);

                fields.push(BlittableField {
                    name: field.name.clone(),
                    offset: aligned_offset,
                    primitive: *p,
                });

                (aligned_offset + size, fields)
            });

    let max_align = def
        .fields
        .iter()
        .filter_map(|f| match &f.type_expr {
            TypeExpr::Primitive(p) => p.alignment(),
            _ => None,
        })
        .max()
        .unwrap_or(1);

    let size = align_up(final_offset, max_align);
    (size, fields)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
    use crate::ir::definitions::{
        CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, FieldDef,
        FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, FieldName, FunctionId, MethodId, ParamName, RecordId,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use boltffi_ffi_rules::naming;

    fn test_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog: TypeCatalog::default(),
            functions: vec![],
        }
    }

    fn lowerer_for_contract(contract: &FfiContract) -> Lowerer<'_> {
        Lowerer::new(contract)
    }

    #[test]
    fn param_strategy_primitive_is_direct() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Primitive(PrimitiveType::I32),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Scalar(ScalarOrigin::Primitive(PrimitiveType::I32))
        ));
    }

    #[test]
    fn param_strategy_string_is_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(&TypeExpr::String, &ParamPassing::Ref);

        assert!(matches!(strategy, Transport::Span(SpanContent::Utf8)));
    }

    #[test]
    fn param_strategy_vec_primitive_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::F32))),
            &ParamPassing::Ref,
        );

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::F32
            )))
        ));
    }

    #[test]
    fn param_strategy_handle_non_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy =
            lowerer.classify_param(&TypeExpr::Handle(class_id.clone()), &ParamPassing::Value);

        assert!(matches!(
            strategy,
            Transport::Handle { class_id: ref id, nullable: false } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy = lowerer.classify_param(
            &TypeExpr::Option(Box::new(TypeExpr::Handle(class_id.clone()))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Handle { class_id: ref id, nullable: true } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_callback_impl_trait() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.classify_param(
            &TypeExpr::Callback(callback_id.clone()),
            &ParamPassing::ImplTrait,
        );

        assert!(matches!(
            strategy,
            Transport::Callback {
                callback_id: ref id,
                style: CallbackStyle::ImplTrait,
                nullable: false
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn param_strategy_option_callback_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.classify_param(
            &TypeExpr::Option(Box::new(TypeExpr::Callback(callback_id.clone()))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Callback {
                callback_id: ref id,
                style: CallbackStyle::BoxedDyn,
                nullable: true
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn lower_return_void() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan = lowerer.lower_return(&ReturnDef::Void);

        assert!(matches!(plan, ReturnPlan::Void));
    }

    #[test]
    fn lower_return_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan =
            lowerer.lower_return(&ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool)));

        assert!(matches!(
            plan,
            ReturnPlan::Value(Transport::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::Bool
            )))
        ));
    }

    #[test]
    fn lower_return_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        assert!(matches!(
            plan,
            ReturnPlan::Value(Transport::Handle {
                nullable: false,
                ..
            })
        ));
    }

    #[test]
    fn lower_return_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Option(Box::new(
            TypeExpr::Handle(class_id),
        ))));

        assert!(matches!(
            plan,
            ReturnPlan::Value(Transport::Handle { nullable: true, .. })
        ));
    }

    #[test]
    fn lower_return_result_handle_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: Transport::Handle {
                    nullable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn lower_return_result_callback_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Callback(callback_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: Transport::Callback {
                    nullable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Primitive(PrimitiveType::U64));

        assert!(matches!(codec, CodecPlan::Primitive(PrimitiveType::U64)));
    }

    #[test]
    fn build_codec_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::String);

        assert!(matches!(codec, CodecPlan::String));
    }

    #[test]
    fn build_codec_option() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Option(Box::new(TypeExpr::String)));

        assert!(matches!(codec, CodecPlan::Option(_)));
    }

    #[test]
    fn build_codec_vec_primitive_is_blittable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(
            PrimitiveType::I32,
        ))));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Blittable { element_size: 4 },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_vec_string_is_encoded() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::String)));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Encoded,
                ..
            }
        ));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_handle_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Handle(ClassId::new("Foo")));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_callback_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Callback(CallbackId::new("Bar")));
    }

    #[test]
    fn lower_function_sync() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Ref,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(
            &plan.target,
            CallTarget::GlobalSymbol(s) if s.as_str() == naming::function_ffi_name("greet").as_str()
        ));
        assert_eq!(plan.params.len(), 1);
        assert!(matches!(plan.kind, CallPlanKind::Sync { .. }));
    }

    #[test]
    fn lower_function_async() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("fetch"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(plan.kind, CallPlanKind::Async { .. }));
    }

    #[test]
    fn lower_method_inserts_self_handle() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Client");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("connect"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 1);
        assert!(matches!(
            &plan.params[0].transport,
            Transport::Handle {
                nullable: false,
                ..
            }
        ));
        assert_eq!(plan.params[0].name.as_str(), "self");
    }

    #[test]
    fn lower_method_static_no_self() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Utils");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("helper"),
            receiver: Receiver::Static,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 0);
    }

    #[test]
    fn lower_constructor_non_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Builder");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Value(Transport::Handle {
                    nullable: false,
                    ..
                })
            }
        ));
    }

    #[test]
    fn lower_constructor_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Parser");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("try_new"),
            is_fallible: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { .. }
            }
        ));
    }

    #[test]
    fn lower_callback_uses_vtable_field() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("EventHandler"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("on_event"),
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans.len(), 1);
        assert!(matches!(
            &plans[0].target,
            CallTarget::VtableField(id) if id.as_str() == naming::vtable_field_name("on_event").as_str()
        ));
    }

    #[test]
    fn lower_callback_inserts_callback_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("notify"),
                params: vec![ParamDef {
                    name: ParamName::new("msg"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Ref,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans[0].params.len(), 2);
        assert_eq!(plans[0].params[0].name.as_str(), "callback");
        assert!(matches!(
            &plans[0].params[0].transport,
            Transport::Callback {
                nullable: false,
                ..
            }
        ));
    }

    #[test]
    fn blittable_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Blittable { size: 8, .. }));
    }

    #[test]
    fn encoded_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Person");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("age"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Encoded { .. }));
    }

    #[test]
    fn async_result_handles_result_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Session");
        let async_plan = lowerer.build_async_plan(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id.clone()),
            err: TypeExpr::String,
        });

        match async_plan.result {
            ReturnPlan::Fallible { ok, err_codec } => {
                match ok {
                    Transport::Handle {
                        class_id: id,
                        nullable,
                    } => {
                        assert_eq!(id.as_str(), "Session");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Fallible"),
        }
    }

    #[test]
    fn param_strategy_vec_primitive_owned_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::U8
            )))
        ));
    }

    #[test]
    fn param_strategy_ref_mut_has_mutable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let param = lowerer.lower_param(&ParamDef {
            name: ParamName::new("s"),
            type_expr: TypeExpr::String,
            passing: ParamPassing::RefMut,
            doc: None,
        });

        assert!(matches!(
            param.transport,
            Transport::Span(SpanContent::Utf8)
        ));
        assert_eq!(param.mutability, Mutability::Mutable);
    }

    #[test]
    fn param_strategy_bytes_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(&TypeExpr::Bytes, &ParamPassing::Ref);

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::U8
            )))
        ));
    }

    #[test]
    fn lower_constructor_fallible_verifies_ok_and_err() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("connect"),
            is_fallible: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        match plan.kind {
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { ok, err_codec },
            } => {
                match ok {
                    Transport::Handle {
                        class_id: id,
                        nullable,
                    } => {
                        assert_eq!(id.as_str(), "Connection");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle in ok"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Sync Fallible"),
        }
    }

    #[test]
    fn blittable_record_layout_verifies_offsets() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Packed");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Blittable { size, fields } => {
                assert_eq!(size, 12);
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "a");
                assert_eq!(fields[0].offset, 0);
                assert_eq!(fields[0].primitive, PrimitiveType::U8);
                assert_eq!(fields[1].name.as_str(), "b");
                assert_eq!(fields[1].offset, 4);
                assert_eq!(fields[1].primitive, PrimitiveType::U32);
                assert_eq!(fields[2].name.as_str(), "c");
                assert_eq!(fields[2].offset, 8);
                assert_eq!(fields[2].primitive, PrimitiveType::U8);
            }
            _ => panic!("expected Blittable"),
        }
    }

    #[test]
    fn vec_blittable_record_is_blittable() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Vec2");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Record(record_id))));

        match codec {
            CodecPlan::Vec { element, layout } => {
                assert!(matches!(layout, VecLayout::Blittable { element_size: 16 }));
                assert!(matches!(*element, CodecPlan::Record { .. }));
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn build_codec_result() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Result {
            ok: Box::new(TypeExpr::Primitive(PrimitiveType::I64)),
            err: Box::new(TypeExpr::String),
        });

        match codec {
            CodecPlan::Result { ok, err } => {
                assert!(matches!(*ok, CodecPlan::Primitive(PrimitiveType::I64)));
                assert!(matches!(*err, CodecPlan::String));
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn lower_return_verifies_class_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Database");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        match plan {
            ReturnPlan::Value(Transport::Handle { class_id, nullable }) => {
                assert_eq!(class_id.as_str(), "Database");
                assert!(!nullable);
            }
            _ => panic!("expected Value Handle"),
        }
    }

    #[test]
    fn lower_callback_verifies_callback_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("MyCallback"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("invoke"),
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        match &plans[0].params[0].transport {
            Transport::Callback {
                callback_id,
                style,
                nullable,
            } => {
                assert_eq!(callback_id.as_str(), "MyCallback");
                assert_eq!(*style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback strategy"),
        }
    }

    #[test]
    fn param_strategy_callback_boxed_dyn() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let strategy = lowerer.classify_param(
            &TypeExpr::Callback(callback_id.clone()),
            &ParamPassing::BoxedDyn,
        );

        match strategy {
            Transport::Callback {
                callback_id: id,
                style,
                nullable,
            } => {
                assert_eq!(id.as_str(), "Handler");
                assert_eq!(style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback"),
        }
    }

    #[test]
    fn lower_method_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Service");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("start"),
            receiver: Receiver::RefMutSelf,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(
                    s.as_str(),
                    naming::method_ffi_name("Service", "start").as_str()
                );
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn lower_constructor_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Factory");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();

        let default_ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &default_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(s.as_str(), naming::class_ffi_new("Factory").as_str())
            }
            _ => panic!("expected GlobalSymbol"),
        }

        let named_ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("with_config"),
            is_fallible: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &named_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(
                    s.as_str(),
                    naming::method_ffi_name("Factory", "with_config").as_str()
                )
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn encoded_record_verifies_field_codecs() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Message");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("id"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("body"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("tags"),
                    type_expr: TypeExpr::Vec(Box::new(TypeExpr::String)),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Encoded { fields } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "id");
                assert!(matches!(
                    fields[0].codec,
                    CodecPlan::Primitive(PrimitiveType::U64)
                ));
                assert_eq!(fields[1].name.as_str(), "body");
                assert!(matches!(fields[1].codec, CodecPlan::String));
                assert_eq!(fields[2].name.as_str(), "tags");
                assert!(matches!(fields[2].codec, CodecPlan::Vec { .. }));
            }
            _ => panic!("expected Encoded"),
        }
    }

    #[test]
    fn string_param_produces_synthetic_len() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let abi = lowerer.abi_call_for_function(&func);

        assert_eq!(abi.params.len(), 2);
        assert!(matches!(
            abi.params[0].role,
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Utf8),
                ..
            }
        ));
        assert_eq!(abi.params[0].name.as_str(), "name");
        match &abi.params[1].role {
            ParamRole::SyntheticLen { for_param } => {
                assert_eq!(for_param.as_str(), "name");
            }
            other => panic!("expected SyntheticLen, got {:?}", other),
        }
    }

    #[test]
    fn blittable_record_param_produces_composite() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("move_to"),
            params: vec![ParamDef {
                name: ParamName::new("point"),
                type_expr: TypeExpr::Record(record_id),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let abi = lowerer.abi_call_for_function(&func);

        assert_eq!(abi.params.len(), 1);
        assert!(matches!(
            abi.params[0].role,
            ParamRole::Input {
                transport: Transport::Composite(_),
                len_param: None,
                decode_ops: Some(_),
                encode_ops: Some(_),
                ..
            }
        ));
        assert_eq!(abi.params[0].name.as_str(), "point");
        assert_eq!(
            abi.params[0].abi_type,
            AbiType::Struct(RecordId::new("Point"))
        );
    }

    #[test]
    fn fallible_constructor_produces_nullable_handle_not_panic() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![ConstructorDef::Default {
                params: vec![ParamDef {
                    name: ParamName::new("url"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                is_fallible: true,
                doc: None,
                deprecated: None,
            }],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let abi = lowerer.abi_call_for_constructor(class, &class.constructors[0], 0);

        assert!(matches!(
            abi.returns.transport,
            Some(Transport::Handle { nullable: true, .. })
        ));
        assert!(matches!(abi.error, ErrorTransport::Encoded { .. }));
    }

    fn contract_with_closure(
        callback_id: &str,
        params: Vec<ParamDef>,
        returns: ReturnDef,
    ) -> FfiContract {
        let mut contract = test_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new(callback_id),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params,
                returns,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        });
        contract
    }

    #[test]
    fn closure_void_return_yields_void_abi_type() {
        let contract = contract_with_closure(
            "__Closure_I32",
            vec![ParamDef {
                name: ParamName::new("x"),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Void,
        );
        let lowerer = lowerer_for_contract(&contract);
        let (params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_I32"));
        assert_eq!(params, vec![AbiType::I32]);
        assert_eq!(ret, AbiType::Void);
    }

    #[test]
    fn closure_primitive_return_yields_primitive_abi_type() {
        let contract = contract_with_closure(
            "__Closure_I32ToI32",
            vec![ParamDef {
                name: ParamName::new("x"),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        );
        let lowerer = lowerer_for_contract(&contract);
        let (params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_I32ToI32"));
        assert_eq!(params, vec![AbiType::I32]);
        assert_eq!(ret, AbiType::I32);
    }

    #[test]
    fn closure_blittable_record_return_yields_struct_abi_type() {
        let mut contract = contract_with_closure(
            "__Closure_PointToPoint",
            vec![ParamDef {
                name: ParamName::new("p"),
                type_expr: TypeExpr::Record(RecordId::new("Point")),
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("Point"))),
        );
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("Point"),
            is_repr_c: true,
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });
        let lowerer = lowerer_for_contract(&contract);
        let (_params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_PointToPoint"));
        assert_eq!(ret, AbiType::Struct(RecordId::new("Point")));
    }

    #[test]
    fn closure_string_return_yields_pointer_abi_type() {
        let contract = contract_with_closure(
            "__Closure_StringToString",
            vec![ParamDef {
                name: ParamName::new("s"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Value(TypeExpr::String),
        );
        let lowerer = lowerer_for_contract(&contract);
        let (_params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_StringToString"));
        assert_eq!(ret, AbiType::Pointer(PrimitiveType::U8));
    }
}
