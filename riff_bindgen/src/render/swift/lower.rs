use heck::ToLowerCamelCase;
use riff_ffi_rules::naming::{
    self, snake_to_camel as camel_case, to_upper_camel_case as pascal_case,
};

use std::collections::HashMap;

use crate::ir::AbiContract;
use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiParam, AbiStream, AsyncResultTransport, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnTransport, StreamItemTransport,
};
use crate::ir::plan::{CallbackStyle, Mutability};
use crate::ir::codec::{CodecPlan, EnumLayout, RecordLayout, VariantPayloadLayout, VecLayout};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, ConstructorDef, EnumRepr, ParamDef, Receiver, ReturnDef, StreamMode,
};
use crate::ir::ids::{CallbackId, ClassId, EnumId, ParamName, RecordId};
use crate::ir::plan::AbiType;
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::codec;
use super::plan::{
    SwiftAsyncConversion, SwiftAsyncResult, SwiftCallback, SwiftCallbackMethod, SwiftCallbackParam,
    SwiftCallMode, SwiftClass, SwiftClosureTrampoline, SwiftClosureTrampolineParam,
    SwiftConstructor, SwiftConversion, SwiftEnum, SwiftField, SwiftFunction, SwiftMethod,
    SwiftModule, SwiftParam, SwiftRecord, SwiftReturn, SwiftStream, SwiftStreamMode, SwiftVariant,
    SwiftVariantPayload,
};

struct AbiIndex {
    calls: HashMap<CallId, usize>,
    callbacks: HashMap<CallbackId, usize>,
    record_codecs: HashMap<RecordId, usize>,
    enum_codecs: HashMap<EnumId, usize>,
}

impl AbiIndex {
    fn new(contract: &AbiContract) -> Self {
        let calls = contract
            .calls
            .iter()
            .enumerate()
            .map(|(index, call)| (call.id.clone(), index))
            .collect();
        let callbacks = contract
            .callbacks
            .iter()
            .enumerate()
            .map(|(index, callback)| (callback.callback_id.clone(), index))
            .collect();
        let record_codecs = contract
            .record_codecs
            .iter()
            .enumerate()
            .map(|(index, (id, _))| (id.clone(), index))
            .collect();
        let enum_codecs = contract
            .enum_codecs
            .iter()
            .enumerate()
            .map(|(index, (id, _))| (id.clone(), index))
            .collect();

        Self {
            calls,
            callbacks,
            record_codecs,
            enum_codecs,
        }
    }

    fn call<'a>(&self, contract: &'a AbiContract, id: &CallId) -> &'a AbiCall {
        let index = self.calls.get(id).expect("abi call should exist");
        &contract.calls[*index]
    }

    fn callback<'a>(
        &self,
        contract: &'a AbiContract,
        id: &CallbackId,
    ) -> &'a AbiCallbackInvocation {
        let index = self.callbacks.get(id).expect("abi callback should exist");
        &contract.callbacks[*index]
    }

    fn record_codec<'a>(&self, contract: &'a AbiContract, id: &RecordId) -> &'a CodecPlan {
        let index = self.record_codecs.get(id).expect("record codec should exist");
        &contract.record_codecs[*index].1
    }

    fn enum_codec<'a>(&self, contract: &'a AbiContract, id: &EnumId) -> &'a CodecPlan {
        let index = self.enum_codecs.get(id).expect("enum codec should exist");
        &contract.enum_codecs[*index].1
    }
}

pub struct SwiftLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    abi_index: AbiIndex,
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    pub fn new(contract: &'a FfiContract, abi: &'a AbiContract) -> Self {
        Self {
            contract,
            abi,
            abi_index: AbiIndex::new(abi),
        }
    }

    pub fn lower(self) -> SwiftModule {
        let records = self.lower_records();
        let enums = self.lower_enums();
        let classes = self.lower_classes();
        let callbacks = self.lower_callbacks();
        let functions = self.lower_functions();

        SwiftModule {
            imports: vec!["Foundation".to_string()],
            records,
            enums,
            classes,
            callbacks,
            functions,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Records
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_records(&self) -> Vec<SwiftRecord> {
        self.contract
            .catalog
            .all_records()
            .map(|def| {
                let codec = self.abi_index.record_codec(self.abi, &def.id);

                let layout = match codec {
                    CodecPlan::Record { layout, .. } => layout,
                    _ => panic!("expected CodecPlan::Record"),
                };

                let (fields, blittable_size) = match layout {
                    RecordLayout::Encoded { fields } => (
                        fields
                            .iter()
                            .map(|f| SwiftField {
                                swift_name: camel_case(f.name.as_str()),
                                swift_type: codec::swift_type(&f.codec),
                                default_expr: None,
                                codec: f.codec.clone(),
                                c_offset: None,
                            })
                            .collect(),
                        None,
                    ),
                    RecordLayout::Blittable { fields, size } => (
                        fields
                            .iter()
                            .map(|f| SwiftField {
                                swift_name: camel_case(f.name.as_str()),
                                swift_type: codec::swift_primitive(f.primitive),
                                default_expr: None,
                                codec: CodecPlan::Primitive(f.primitive),
                                c_offset: Some(f.offset),
                            })
                            .collect(),
                        Some(*size),
                    ),
                    RecordLayout::Recursive => (vec![], None),
                };

                SwiftRecord {
                    class_name: self.swift_name_for_record(&def.id),
                    fields,
                    is_blittable: layout.is_blittable(),
                    blittable_size,
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enums
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_enums(&self) -> Vec<SwiftEnum> {
        self.contract
            .catalog
            .all_enums()
            .map(|def| {
                let codec = self.abi_index.enum_codec(self.abi, &def.id);

                let layout = match codec {
                    CodecPlan::Enum { layout, .. } => layout,
                    _ => panic!("expected CodecPlan::Enum"),
                };

                let (is_c_style, variants) = match layout {
                    EnumLayout::CStyle { .. } => (
                        true,
                        match &def.repr {
                            EnumRepr::CStyle { variants, .. } => variants
                                .iter()
                                .map(|v| SwiftVariant {
                                    swift_name: v.name.as_str().to_lower_camel_case(),
                                    discriminant: v.discriminant,
                                    payload: SwiftVariantPayload::Unit,
                                })
                                .collect(),
                            _ => vec![],
                        },
                    ),
                    EnumLayout::Data { variants, .. } => (
                        false,
                        variants
                            .iter()
                            .map(|v| SwiftVariant {
                                swift_name: v.name.as_str().to_lower_camel_case(),
                                discriminant: v.discriminant,
                                payload: self.lower_variant_payload_layout(&v.payload),
                            })
                            .collect(),
                    ),
                    EnumLayout::Recursive => (false, vec![]),
                };

                SwiftEnum {
                    name: self.swift_name_for_enum(&def.id),
                    variants,
                    is_c_style,
                    is_error: def.is_error,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_variant_payload_layout(&self, payload: &VariantPayloadLayout) -> SwiftVariantPayload {
        match payload {
            VariantPayloadLayout::Unit => SwiftVariantPayload::Unit,
            VariantPayloadLayout::Fields(fields) => SwiftVariantPayload::Struct(
                fields
                    .iter()
                    .map(|f| SwiftField {
                        swift_name: camel_case(f.name.as_str()),
                        swift_type: codec::swift_type(&f.codec),
                        default_expr: None,
                        codec: f.codec.clone(),
                        c_offset: None,
                    })
                    .collect(),
            ),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Classes
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_classes(&self) -> Vec<SwiftClass> {
        self.contract
            .catalog
            .all_classes()
            .map(|def| {
                let class_name = self.swift_name_for_class(&def.id);
                let ffi_free = naming::class_ffi_free(def.id.as_str()).to_string();

                let constructors = def
                    .constructors
                    .iter()
                    .enumerate()
                    .map(|(idx, ctor)| {
                        let call = self.abi_call(&CallId::Constructor {
                            class_id: def.id.clone(),
                            index: idx,
                        });

                        match ctor {
                            ConstructorDef::Default {
                                is_fallible, doc, ..
                            } => SwiftConstructor::Designated {
                                ffi_symbol: call.symbol.as_str().to_string(),
                                params: ctor
                                    .params()
                                    .into_iter()
                                    .map(|p| self.lower_param(p, call))
                                    .collect(),
                                is_fallible: *is_fallible,
                                doc: doc.clone(),
                            },
                            ConstructorDef::NamedFactory {
                                name,
                                is_fallible,
                                doc,
                                ..
                            } => SwiftConstructor::Factory {
                                name: camel_case(name.as_str()),
                                ffi_symbol: call.symbol.as_str().to_string(),
                                is_fallible: *is_fallible,
                                doc: doc.clone(),
                            },
                            ConstructorDef::NamedInit {
                                name,
                                first_param,
                                rest_params,
                                is_fallible,
                                doc,
                                ..
                            } => {
                                let label = camel_case(name.as_str());
                                let mut first = self.lower_param(first_param, call);
                                first.label = Some(label.clone());
                                let rest = rest_params
                                    .iter()
                                    .map(|p| self.lower_param(p, call));
                                SwiftConstructor::Convenience {
                                    name: label,
                                    ffi_symbol: call.symbol.as_str().to_string(),
                                    params: std::iter::once(first).chain(rest).collect(),
                                    is_fallible: *is_fallible,
                                    doc: doc.clone(),
                                }
                            }
                        }
                    })
                    .collect();

                let methods = def
                    .methods
                    .iter()
                    .map(|method| {
                        let call = self.abi_call(&CallId::Method {
                            class_id: def.id.clone(),
                            method_id: method.id.clone(),
                        });
                        let mode = self.lower_call_mode(call);
                        let returns = match &call.mode {
                            CallMode::Async(async_call) => {
                                self.lower_return_def_for_async(&async_call.error)
                            }
                            CallMode::Sync => self.lower_return_def(&method.returns),
                        };

                        SwiftMethod {
                            name: camel_case(method.id.as_str()),
                            mode,
                            params: method
                                .params
                                .iter()
                                .map(|p| self.lower_param(p, call))
                                .collect(),
                            returns,
                            is_static: method.receiver == Receiver::Static,
                            doc: method.doc.clone(),
                        }
                    })
                    .collect();

                let streams = self.lower_streams_for_class(&def.id, &class_name);

                SwiftClass {
                    name: class_name,
                    ffi_free,
                    constructors,
                    methods,
                    streams,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_streams_for_class(&self, class_id: &ClassId, class_name: &str) -> Vec<SwiftStream> {
        self.abi
            .streams
            .iter()
            .filter(|s| &s.class_id == class_id)
            .map(|stream| self.lower_stream(stream, class_name))
            .collect()
    }

    fn lower_stream(&self, stream: &AbiStream, class_name: &str) -> SwiftStream {
        let StreamItemTransport::WireEncoded { codec } = &stream.item;
        let method_name_pascal = pascal_case(stream.stream_id.as_str());

        let mode = match stream.mode {
            StreamMode::Async => SwiftStreamMode::Async,
            StreamMode::Batch => SwiftStreamMode::Batch {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
            StreamMode::Callback => SwiftStreamMode::Callback {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
        };

        SwiftStream {
            name: camel_case(stream.stream_id.as_str()),
            mode,
            item_type: codec::swift_type(codec),
            item_decode_expr: codec::decode_stream_item(codec),
            subscribe: stream.subscribe.to_string(),
            poll: stream.poll.to_string(),
            pop_batch: stream.pop_batch.to_string(),
            wait: stream.wait.to_string(),
            unsubscribe: stream.unsubscribe.to_string(),
            free: stream.free.to_string(),
            free_buf: self.abi.free_buf.to_string(),
            atomic_cas: self.abi.atomic_cas.to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Callbacks
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_callbacks(&self) -> Vec<SwiftCallback> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|def| def.kind == CallbackKind::Trait)
            .map(|def| {
                let protocol_name = pascal_case(def.id.as_str());
                let vtable_var = format!("{}VTableInstance", lower_first_char(&protocol_name));
                let plan = self.abi_index.callback(self.abi, &def.id);

                let methods = plan
                    .methods
                    .iter()
                    .map(|method| {
                        let returns = self.swift_return_from_abi(&method.return_, &method.error);
                        let has_out_param = !method.is_async && !returns.is_void();

                        SwiftCallbackMethod {
                            swift_name: camel_case(method.id.as_str()),
                            ffi_name: method.vtable_field.as_str().to_string(),
                            params: method
                                .params
                                .iter()
                                .filter(|p| {
                                    matches!(p.role, ParamRole::InDirect | ParamRole::InEncoded { .. })
                                })
                                .map(|p| self.lower_callback_param(p))
                                .collect(),
                            returns,
                            is_async: method.is_async,
                            has_out_param,
                        }
                    })
                    .collect();

                SwiftCallback {
                    protocol_name: protocol_name.clone(),
                    wrapper_class: format!("{}Wrapper", protocol_name),
                    vtable_var,
                    vtable_type: plan.vtable_type.as_str().to_string(),
                    bridge_name: format!("{}Bridge", protocol_name),
                    register_fn: plan.register_fn.as_str().to_string(),
                    create_fn: plan.create_fn.as_str().to_string(),
                    methods,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_callback_param(&self, param: &AbiParam) -> SwiftCallbackParam {
        let label = camel_case(param.name.as_str());
        let (swift_type, ffi_args, decode_prelude) = match &param.role {
            ParamRole::InDirect => (
                self.abi_to_swift(param.ffi_type),
                vec![label.clone()],
                None,
            ),
            ParamRole::InEncoded { codec, .. } => {
                let len_name = format!("{}Len", label);
                (
                    codec::swift_type(codec),
                    vec![label.clone(), len_name.clone()],
                    Some(format!(
                        "let {} = {{ let wire = WireBuffer(ptr: {}!, len: Int({})); var pos = 0; return {} }}()",
                        label,
                        label,
                        len_name,
                        codec::decode_inline(codec)
                    )),
                )
            }
            _ => panic!("unsupported ABI param role for Swift callback"),
        };

        SwiftCallbackParam {
            label: label.clone(),
            swift_type,
            call_arg: label,
            ffi_args,
            decode_prelude,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_functions(&self) -> Vec<SwiftFunction> {
        self.contract
            .functions
            .iter()
            .map(|def| {
                let call = self.abi_call(&CallId::Function(def.id.clone()));
                let mode = self.lower_call_mode(call);
                let returns = match &call.mode {
                    CallMode::Async(async_call) => {
                        self.lower_return_def_for_async(&async_call.error)
                    }
                    CallMode::Sync => self.lower_return_def(&def.returns),
                };

                SwiftFunction {
                    name: camel_case(def.id.as_str()),
                    mode,
                    params: def
                        .params
                        .iter()
                        .map(|p| self.lower_param(p, call))
                        .collect(),
                    returns,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Params
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_param(&self, param: &ParamDef, call: &AbiCall) -> SwiftParam {
        let abi_param = self.abi_param_for_semantic(call, &param.name);
        let swift_name = camel_case(param.name.as_str());

        let (swift_type, conversion) = match &abi_param.role {
            ParamRole::InDirect => (self.swift_type(&param.type_expr), SwiftConversion::Direct),
            ParamRole::InBuffer { element_abi, mutability, .. } => {
                let element_type = self.abi_to_swift(*element_abi);
                if *element_abi == AbiType::U8 && *mutability == Mutability::Shared {
                    ("Data".to_string(), SwiftConversion::ToData)
                } else {
                    let conversion = match mutability {
                        Mutability::Mutable => SwiftConversion::MutableBuffer {
                            element_type: element_type.clone(),
                        },
                        Mutability::Shared => SwiftConversion::PrimitiveBuffer {
                            element_type: element_type.clone(),
                        },
                    };
                    (format!("[{}]", element_type), conversion)
                }
            }
            ParamRole::InString { .. } => ("String".to_string(), SwiftConversion::ToString),
            ParamRole::InEncoded { codec, .. } => (
                codec::swift_type(codec),
                SwiftConversion::ToWireBuffer { codec: codec.clone() },
            ),
            ParamRole::InHandle { class_id, nullable } => {
                let class_name = self.swift_name_for_class(class_id);
                let swift_type = if *nullable {
                    format!("{}?", class_name)
                } else {
                    class_name.clone()
                };
                (
                    swift_type,
                    SwiftConversion::PassHandle {
                        class_name,
                        nullable: *nullable,
                    },
                )
            }
            ParamRole::InCallback { callback_id, nullable, style } => {
                match style {
                    CallbackStyle::BoxedDyn => {
                        let protocol = pascal_case(callback_id.as_str());
                        let swift_type = if *nullable {
                            format!("(any {})?", protocol)
                        } else {
                            format!("any {}", protocol)
                        };
                        (
                            swift_type,
                            SwiftConversion::WrapCallback { protocol },
                        )
                    }
                    CallbackStyle::ImplTrait => {
                        let closure_plan = self.build_closure_trampoline(callback_id, &swift_name);
                        let swift_type = format!("@escaping {}", closure_plan.swift_type);
                        (
                            swift_type,
                            SwiftConversion::InlineClosure { closure: closure_plan },
                        )
                    }
                }
            }
            ParamRole::OutBuffer { codec, .. } => {
                let element_type = match codec {
                    CodecPlan::Vec { element, .. } => codec::swift_type(element),
                    _ => codec::swift_type(codec),
                };
                (
                    format!("[{}]", element_type),
                    SwiftConversion::MutableBuffer { element_type },
                )
            }
            _ => panic!("unsupported ABI param role for Swift param: {:?}", abi_param.role),
        };

        SwiftParam {
            label: None,
            name: swift_name,
            swift_type,
            conversion,
        }
    }

    fn abi_param_for_semantic<'b>(&self, call: &'b AbiCall, name: &ParamName) -> &'b AbiParam {
        call.params
            .iter()
            .find(|param| {
                param.name.as_str() == name.as_str()
                    && matches!(
                        param.role,
                        ParamRole::InDirect
                            | ParamRole::InBuffer { .. }
                            | ParamRole::InString { .. }
                            | ParamRole::InEncoded { .. }
                            | ParamRole::OutBuffer { .. }
                            | ParamRole::InHandle { .. }
                            | ParamRole::InCallback { .. }
                    )
            })
            .expect("ABI param should exist")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Returns
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_return_def(&self, returns: &ReturnDef) -> SwiftReturn {
        match returns {
            ReturnDef::Void => SwiftReturn::Void,
            ReturnDef::Value(ty) => self.swift_return_from_type_expr(ty),
            ReturnDef::Result { ok, err } => SwiftReturn::Throws {
                ok: Box::new(self.swift_return_from_type_expr(ok)),
                err_type: self.swift_type(err),
            },
        }
    }

    fn swift_return_from_abi(&self, return_: &ReturnTransport, error: &ErrorTransport) -> SwiftReturn {
        let base = match return_ {
            ReturnTransport::Void => SwiftReturn::Void,
            ReturnTransport::Direct(abi) => SwiftReturn::Direct {
                swift_type: self.abi_to_swift(*abi),
            },
            ReturnTransport::Encoded { codec } => SwiftReturn::FromWireBuffer {
                swift_type: codec::swift_type(codec),
                codec: codec.clone(),
            },
            ReturnTransport::Handle { class_id, nullable } => {
                let class_name = self.swift_name_for_class(class_id);
                SwiftReturn::Handle { class_name, nullable: *nullable }
            }
            ReturnTransport::Callback { callback_id, nullable } => {
                let protocol = pascal_case(callback_id.as_str());
                let swift_type = if *nullable {
                    format!("(any {})?", protocol)
                } else {
                    format!("any {}", protocol)
                };
                SwiftReturn::Direct { swift_type }
            }
        };

        match error {
            ErrorTransport::None => base,
            ErrorTransport::Encoded { codec } => SwiftReturn::Throws {
                ok: Box::new(base),
                err_type: codec::swift_type(codec),
            },
            ErrorTransport::StatusCode => SwiftReturn::Throws {
                ok: Box::new(base),
                err_type: "FfiError".to_string(),
            },
        }
    }

    fn swift_return_from_type_expr(&self, ty: &TypeExpr) -> SwiftReturn {
        match ty {
            TypeExpr::Void => SwiftReturn::Void,
            TypeExpr::Primitive(p) => SwiftReturn::Direct {
                swift_type: codec::swift_primitive(*p),
            },
            TypeExpr::Handle(id) => SwiftReturn::Handle {
                class_name: self.swift_name_for_class(id),
                nullable: false,
            },
            TypeExpr::Callback(id) => {
                let protocol = pascal_case(id.as_str());
                SwiftReturn::Direct { swift_type: format!("any {}", protocol) }
            }
            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(id) => SwiftReturn::Handle {
                    class_name: self.swift_name_for_class(id),
                    nullable: true,
                },
                TypeExpr::Callback(id) => {
                    let protocol = pascal_case(id.as_str());
                    SwiftReturn::Direct { swift_type: format!("(any {})?", protocol) }
                }
                _ => {
                    let codec = self.codec_for_type_expr(ty);
                    SwiftReturn::FromWireBuffer {
                        swift_type: codec::swift_type(&codec),
                        codec,
                    }
                }
            },
            _ => {
                let codec = self.codec_for_type_expr(ty);
                SwiftReturn::FromWireBuffer {
                    swift_type: codec::swift_type(&codec),
                    codec,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type Helpers
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn abi_call(&self, id: &CallId) -> &AbiCall {
        self.abi_index.call(self.abi, id)
    }

    fn swift_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Handle(id) => self.swift_name_for_class(id),
            TypeExpr::Callback(id) => format!("any {}", pascal_case(id.as_str())),
            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(id) => format!("{}?", self.swift_name_for_class(id)),
                TypeExpr::Callback(id) => format!("(any {})?", pascal_case(id.as_str())),
                _ => codec::swift_type(&self.codec_for_type_expr(ty)),
            },
            _ => codec::swift_type(&self.codec_for_type_expr(ty)),
        }
    }

    fn codec_for_type_expr(&self, ty: &TypeExpr) -> CodecPlan {
        match ty {
            TypeExpr::Void => CodecPlan::Void,
            TypeExpr::Primitive(p) => CodecPlan::Primitive(*p),
            TypeExpr::String => CodecPlan::String,
            TypeExpr::Bytes => CodecPlan::Bytes,
            TypeExpr::Builtin(id) => CodecPlan::Builtin(id.clone()),
            TypeExpr::Option(inner) => CodecPlan::Option(Box::new(self.codec_for_type_expr(inner))),
            TypeExpr::Vec(inner) => CodecPlan::Vec {
                element: Box::new(self.codec_for_type_expr(inner)),
                layout: self.vec_layout(inner),
            },
            TypeExpr::Result { ok, err } => CodecPlan::Result {
                ok: Box::new(self.codec_for_type_expr(ok)),
                err: Box::new(self.codec_for_type_expr(err)),
            },
            TypeExpr::Record(id) => self.abi_index.record_codec(self.abi, id).clone(),
            TypeExpr::Enum(id) => self.abi_index.enum_codec(self.abi, id).clone(),
            TypeExpr::Custom(id) => {
                let def = self
                    .contract
                    .catalog
                    .resolve_custom(id)
                    .expect("custom type should be resolved");
                CodecPlan::Custom {
                    id: id.clone(),
                    underlying: Box::new(self.codec_for_type_expr(&def.repr)),
                }
            }
            TypeExpr::Handle(_) | TypeExpr::Callback(_) => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    fn vec_layout(&self, element: &TypeExpr) -> VecLayout {
        match element {
            TypeExpr::Primitive(p) => match p.size_bytes() {
                Some(size) => VecLayout::Blittable { element_size: size },
                None => VecLayout::Encoded,
            },
            TypeExpr::Record(id) => {
                let codec = self.abi_index.record_codec(self.abi, id);
                match codec {
                    CodecPlan::Record {
                        layout: RecordLayout::Blittable { size, .. },
                        ..
                    } => VecLayout::Blittable { element_size: *size },
                    _ => VecLayout::Encoded,
                }
            }
            _ => VecLayout::Encoded,
        }
    }

    fn build_closure_trampoline(
        &self,
        callback_id: &CallbackId,
        param_name: &str,
    ) -> SwiftClosureTrampoline {
        let callback_def = self
            .contract
            .catalog
            .resolve_callback(callback_id)
            .expect("closure callback should exist");
        let method = &callback_def.methods[0];

        let param_types: Vec<String> = method
            .params
            .iter()
            .map(|p| self.swift_type(&p.type_expr))
            .collect();
        let return_type = match &method.returns {
            ReturnDef::Void => "Void".to_string(),
            ReturnDef::Value(ty) => self.swift_type(ty),
            ReturnDef::Result { ok, .. } => format!("Result<{}, Error>", self.swift_type(ok)),
        };

        let swift_type = if param_types.is_empty() {
            format!("() -> {}", return_type)
        } else {
            format!("({}) -> {}", param_types.join(", "), return_type)
        };

        let upper_name = pascal_case(param_name);
        let type_alias = format!("{}CallbackFn", upper_name);
        let box_class = format!("{}CallbackBox", upper_name);

        let trampoline_params: Vec<SwiftClosureTrampolineParam> = method
            .params
            .iter()
            .enumerate()
            .map(|(idx, p)| self.build_closure_trampoline_param(idx, &p.type_expr))
            .collect();

        SwiftClosureTrampoline {
            type_alias,
            swift_type,
            box_class,
            box_var: format!("{}Box", param_name),
            ptr_var: format!("{}Ptr", param_name),
            trampoline_var: format!("{}Trampoline", param_name),
            param_name: param_name.to_string(),
            trampoline_params,
        }
    }

    fn build_closure_trampoline_param(
        &self,
        idx: usize,
        ty: &TypeExpr,
    ) -> SwiftClosureTrampolineParam {
        let is_wire_encoded = self.is_wire_encoded_type(ty);

        if is_wire_encoded {
            let codec = self.codec_for_type_expr(ty);
            let wire_buffer = format!("WireBuffer(ptr: ptr{}!, len: Int(len{}))", idx, idx);
            let decode_expr = codec::decode_with_wire_buffer(&codec, &wire_buffer);
            SwiftClosureTrampolineParam {
                name: format!("ptr{}, len{}", idx, idx),
                c_type: "UnsafePointer<UInt8>?, UInt".to_string(),
                decode_expr,
            }
        } else {
            SwiftClosureTrampolineParam {
                name: format!("arg{}", idx),
                c_type: self.abi_to_swift(self.abi_type_for(ty)),
                decode_expr: format!("arg{}", idx),
            }
        }
    }

    fn is_wire_encoded_type(&self, ty: &TypeExpr) -> bool {
        matches!(
            ty,
            TypeExpr::String
                | TypeExpr::Bytes
                | TypeExpr::Vec(_)
                | TypeExpr::Option(_)
                | TypeExpr::Result { .. }
                | TypeExpr::Record(_)
                | TypeExpr::Enum(_)
                | TypeExpr::Custom(_)
        )
    }

    fn abi_type_for(&self, ty: &TypeExpr) -> AbiType {
        match ty {
            TypeExpr::Void => AbiType::Void,
            TypeExpr::Primitive(p) => match p {
                PrimitiveType::Bool => AbiType::Bool,
                PrimitiveType::I8 => AbiType::I8,
                PrimitiveType::U8 => AbiType::U8,
                PrimitiveType::I16 => AbiType::I16,
                PrimitiveType::U16 => AbiType::U16,
                PrimitiveType::I32 => AbiType::I32,
                PrimitiveType::U32 => AbiType::U32,
                PrimitiveType::I64 => AbiType::I64,
                PrimitiveType::U64 => AbiType::U64,
                PrimitiveType::ISize => AbiType::ISize,
                PrimitiveType::USize => AbiType::USize,
                PrimitiveType::F32 => AbiType::F32,
                PrimitiveType::F64 => AbiType::F64,
            },
            TypeExpr::Handle(_) => AbiType::U64,
            TypeExpr::Callback(_) => AbiType::U64,
            _ => AbiType::Void,
        }
    }

    fn abi_to_swift(&self, abi: AbiType) -> String {
        match abi {
            AbiType::Void => "Void",
            AbiType::Bool => "Bool",
            AbiType::I8 => "Int8",
            AbiType::U8 => "UInt8",
            AbiType::I16 => "Int16",
            AbiType::U16 => "UInt16",
            AbiType::I32 => "Int32",
            AbiType::U32 => "UInt32",
            AbiType::I64 => "Int64",
            AbiType::U64 => "UInt64",
            AbiType::ISize => "Int",
            AbiType::USize => "UInt",
            AbiType::F32 => "Float",
            AbiType::F64 => "Double",
            AbiType::Pointer => "OpaquePointer",
        }
        .to_string()
    }

    fn swift_name_for_record(&self, id: &RecordId) -> String {
        pascal_case(id.as_str())
    }

    fn swift_name_for_enum(&self, id: &EnumId) -> String {
        pascal_case(id.as_str())
    }

    fn swift_name_for_class(&self, id: &ClassId) -> String {
        pascal_case(id.as_str())
    }

    fn lower_call_mode(&self, call: &AbiCall) -> SwiftCallMode {
        match &call.mode {
            CallMode::Sync => SwiftCallMode::Sync {
                symbol: call.symbol.as_str().to_string(),
            },
            CallMode::Async(async_call) => SwiftCallMode::Async {
                start: call.symbol.as_str().to_string(),
                poll: async_call.poll.as_str().to_string(),
                complete: async_call.complete.as_str().to_string(),
                cancel: async_call.cancel.as_str().to_string(),
                free: async_call.free.as_str().to_string(),
                result: self.lower_async_result(&async_call.result, &async_call.error),
            },
        }
    }

    fn lower_async_result(
        &self,
        result: &AsyncResultTransport,
        error: &ErrorTransport,
    ) -> SwiftAsyncResult {
        let throws = !matches!(error, ErrorTransport::None);

        match result {
            AsyncResultTransport::Void => SwiftAsyncResult::Void,
            AsyncResultTransport::Direct(abi) => SwiftAsyncResult::Direct {
                swift_type: self.abi_to_swift(*abi),
                conversion: SwiftAsyncConversion::None,
            },
            AsyncResultTransport::Encoded { codec } => {
                let ok_type = if throws {
                    if let CodecPlan::Result { ok, .. } = codec {
                        Some(codec::swift_type(ok))
                    } else {
                        None
                    }
                } else {
                    None
                };
                SwiftAsyncResult::Encoded {
                    swift_type: codec::swift_type(codec),
                    ok_type,
                    codec: codec.clone(),
                    throws,
                }
            }
            AsyncResultTransport::Handle { class_id, nullable } => SwiftAsyncResult::Direct {
                swift_type: if *nullable {
                    format!("{}?", self.swift_name_for_class(class_id))
                } else {
                    self.swift_name_for_class(class_id)
                },
                conversion: SwiftAsyncConversion::Handle {
                    class_name: self.swift_name_for_class(class_id),
                    nullable: *nullable,
                },
            },
            AsyncResultTransport::Callback { callback_id, nullable } => SwiftAsyncResult::Direct {
                swift_type: if *nullable {
                    format!("(any {})?", pascal_case(callback_id.as_str()))
                } else {
                    format!("any {}", pascal_case(callback_id.as_str()))
                },
                conversion: SwiftAsyncConversion::Callback {
                    protocol: pascal_case(callback_id.as_str()),
                    nullable: *nullable,
                },
            },
        }
    }

    fn lower_return_def_for_async(&self, error: &ErrorTransport) -> SwiftReturn {
        match error {
            ErrorTransport::None => SwiftReturn::Void,
            ErrorTransport::StatusCode => SwiftReturn::Throws {
                ok: Box::new(SwiftReturn::Void),
                err_type: "FfiError".to_string(),
            },
            ErrorTransport::Encoded { codec } => SwiftReturn::Throws {
                ok: Box::new(SwiftReturn::Void),
                err_type: codec::swift_type(codec),
            },
        }
    }
}

fn lower_first_char(name: &str) -> String {
    name.chars()
        .enumerate()
        .map(|(index, ch)| if index == 0 { ch.to_ascii_lowercase() } else { ch })
        .collect()
}
