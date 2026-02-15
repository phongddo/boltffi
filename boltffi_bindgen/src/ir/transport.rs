use boltffi_ffi_rules::naming::{GlobalSymbol, Name};

use crate::ir::{
    AbiCall, AbiContract, AbiParam, AbiType, AsyncCall, CallId, CallMode, CallbackId,
    CallbackStyle, ClassId, ErrorTransport, InputShape, Mutability, OutputShape, ParamName,
    ReadSeq, ValueShape, WriteSeq,
};

#[derive(Debug, Clone)]
pub struct TransportPlan {
    pub calls: Vec<TransportCall>,
}

impl TransportPlan {
    pub fn from_abi(abi: &AbiContract) -> Self {
        let calls = abi.calls.iter().map(TransportCall::from_abi_call).collect();
        Self { calls }
    }
}

#[derive(Debug, Clone)]
pub struct TransportCall {
    pub id: CallId,
    pub symbol: Name<GlobalSymbol>,
    pub inputs: Vec<SyncInputParam>,
    pub mode: TransportCallMode,
    pub error: ErrorTransport,
}

impl TransportCall {
    pub fn from_abi_call(call: &AbiCall) -> Self {
        let inputs = call
            .params
            .iter()
            .filter_map(SyncInputParam::from_abi_param)
            .collect();

        let mode = match &call.mode {
            CallMode::Sync => TransportCallMode::Sync {
                output: SyncOutputAbi::from_abi_call(call),
            },
            CallMode::Async(async_call) => TransportCallMode::Async {
                entry_output: SyncOutputAbi::from_abi_call(call),
                completion: Box::new(AsyncCompletionAbi::from_async_call(async_call)),
            },
        };

        Self {
            id: call.id.clone(),
            symbol: call.symbol.clone(),
            inputs,
            mode,
            error: call.error.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransportCallMode {
    Sync {
        output: SyncOutputAbi,
    },
    Async {
        entry_output: SyncOutputAbi,
        completion: Box<AsyncCompletionAbi>,
    },
}

#[derive(Debug, Clone)]
pub struct AsyncCompletionAbi {
    pub poll: Name<GlobalSymbol>,
    pub complete: Name<GlobalSymbol>,
    pub cancel: Name<GlobalSymbol>,
    pub free: Name<GlobalSymbol>,
    pub output: AsyncOutputAbi,
    pub error: ErrorTransport,
}

impl AsyncCompletionAbi {
    pub fn from_async_call(async_call: &AsyncCall) -> Self {
        Self {
            poll: async_call.poll.clone(),
            complete: async_call.complete.clone(),
            cancel: async_call.cancel.clone(),
            free: async_call.free.clone(),
            output: AsyncOutputAbi::from_async_call(async_call),
            error: async_call.error.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncInputParam {
    pub name: ParamName,
    pub ffi_type: AbiType,
    pub abi: SyncInputAbi,
}

impl SyncInputParam {
    pub fn from_abi_param(param: &AbiParam) -> Option<Self> {
        let abi = match SyncParamAbi::from_abi_param(param) {
            SyncParamAbi::Input(abi) => abi,
            SyncParamAbi::Hidden(_) | SyncParamAbi::UnsupportedValue => return None,
        };
        Some(Self {
            name: param.name.clone(),
            ffi_type: param.ffi_type,
            abi,
        })
    }
}

#[derive(Debug, Clone)]
pub enum SyncParamAbi {
    Input(SyncInputAbi),
    Hidden(SyncHiddenInputAbi),
    UnsupportedValue,
}

impl SyncParamAbi {
    pub fn from_abi_param(param: &AbiParam) -> Self {
        match &param.input_shape {
            InputShape::Value(ValueShape::Scalar(_)) => Self::Input(SyncInputAbi::Scalar),
            InputShape::Utf8Slice { len_param } => Self::Input(SyncInputAbi::Utf8Slice {
                len_param: len_param.clone(),
            }),
            InputShape::PrimitiveSlice {
                len_param,
                mutability,
                element_abi,
            } => Self::Input(SyncInputAbi::PrimitiveSlice {
                len_param: len_param.clone(),
                mutability: *mutability,
                element_abi: *element_abi,
            }),
            InputShape::WirePacket { len_param, value } => Self::Input(SyncInputAbi::WirePacket {
                len_param: len_param.clone(),
                decode_ops: value
                    .read_ops()
                    .unwrap_or_else(|| {
                        panic!(
                            "wire packet input shape missing decode ops for param {}",
                            param.name.as_str()
                        )
                    })
                    .clone(),
                encode_ops: value
                    .write_ops()
                    .unwrap_or_else(|| {
                        panic!(
                            "wire packet input shape missing encode ops for param {}",
                            param.name.as_str()
                        )
                    })
                    .clone(),
            }),
            InputShape::OutputBuffer { len_param, value } => {
                Self::Input(SyncInputAbi::OutputBuffer {
                    len_param: len_param.clone(),
                    decode_ops: value
                        .read_ops()
                        .unwrap_or_else(|| {
                            panic!(
                                "output buffer input shape missing decode ops for param {}",
                                param.name.as_str()
                            )
                        })
                        .clone(),
                })
            }
            InputShape::Handle { class_id, nullable } => Self::Input(SyncInputAbi::Handle {
                class_id: class_id.clone(),
                nullable: *nullable,
            }),
            InputShape::Callback {
                callback_id,
                nullable,
                style,
            } => Self::Input(SyncInputAbi::CallbackHandle {
                callback_id: callback_id.clone(),
                nullable: *nullable,
                style: *style,
            }),
            InputShape::HiddenSyntheticLen { for_param } => {
                Self::Hidden(SyncHiddenInputAbi::SyntheticLen {
                    for_param: for_param.clone(),
                })
            }
            InputShape::HiddenOutLen { for_param } => Self::Hidden(SyncHiddenInputAbi::OutLen {
                for_param: for_param.clone(),
            }),
            InputShape::HiddenOutDirect => Self::Hidden(SyncHiddenInputAbi::OutDirect),
            InputShape::HiddenStatusOut => Self::Hidden(SyncHiddenInputAbi::StatusOut),
            InputShape::Value(_) => Self::UnsupportedValue,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SyncInputAbi {
    Scalar,
    Utf8Slice {
        len_param: ParamName,
    },
    PrimitiveSlice {
        len_param: ParamName,
        mutability: Mutability,
        element_abi: AbiType,
    },
    WirePacket {
        len_param: ParamName,
        decode_ops: ReadSeq,
        encode_ops: WriteSeq,
    },
    OutputBuffer {
        len_param: ParamName,
        decode_ops: ReadSeq,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    CallbackHandle {
        callback_id: CallbackId,
        nullable: bool,
        style: CallbackStyle,
    },
}

impl SyncInputAbi {
    pub fn from_abi_param(param: &AbiParam) -> Option<Self> {
        match SyncParamAbi::from_abi_param(param) {
            SyncParamAbi::Input(abi) => Some(abi),
            SyncParamAbi::Hidden(_) | SyncParamAbi::UnsupportedValue => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SyncHiddenInputAbi {
    SyntheticLen { for_param: ParamName },
    OutLen { for_param: ParamName },
    OutDirect,
    StatusOut,
}

impl SyncHiddenInputAbi {
    pub fn from_abi_param(param: &AbiParam) -> Option<Self> {
        match SyncParamAbi::from_abi_param(param) {
            SyncParamAbi::Hidden(hidden) => Some(hidden),
            SyncParamAbi::Input(_) | SyncParamAbi::UnsupportedValue => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SyncOutputAbi {
    Unit,
    Scalar {
        abi_type: AbiType,
    },
    WirePacket {
        decode_ops: ReadSeq,
        encode_ops: WriteSeq,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    CallbackHandle {
        callback_id: CallbackId,
        nullable: bool,
    },
}

impl SyncOutputAbi {
    pub fn from_abi_call(call: &AbiCall) -> Self {
        Self::from_output_shape(&call.output_shape)
    }

    pub fn from_output_shape(output_shape: &OutputShape) -> Self {
        match output_shape {
            OutputShape::Unit => Self::Unit,
            OutputShape::Value(ValueShape::Scalar(abi_type)) => Self::Scalar {
                abi_type: *abi_type,
            },
            OutputShape::Handle { class_id, nullable } => Self::Handle {
                class_id: class_id.clone(),
                nullable: *nullable,
            },
            OutputShape::Callback {
                callback_id,
                nullable,
            } => Self::CallbackHandle {
                callback_id: callback_id.clone(),
                nullable: *nullable,
            },
            OutputShape::Value(
                ValueShape::OptionScalar { .. }
                | ValueShape::ResultScalar { .. }
                | ValueShape::PrimitiveVec { .. }
                | ValueShape::BlittableRecord { .. }
                | ValueShape::WireEncoded { .. },
            ) => {
                let decode_ops = output_shape_value_read_ops(output_shape);
                let encode_ops = output_shape_value_write_ops(output_shape);
                Self::WirePacket {
                    decode_ops,
                    encode_ops,
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AsyncOutputAbi {
    Unit,
    Scalar {
        abi_type: AbiType,
    },
    WirePacket {
        decode_ops: ReadSeq,
        encode_ops: WriteSeq,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    CallbackHandle {
        callback_id: CallbackId,
        nullable: bool,
    },
}

impl AsyncOutputAbi {
    pub fn from_async_call(async_call: &AsyncCall) -> Self {
        Self::from_result_shape(&async_call.result_shape)
    }

    pub fn from_result_shape(result_shape: &OutputShape) -> Self {
        match result_shape {
            OutputShape::Unit => Self::Unit,
            OutputShape::Value(ValueShape::Scalar(abi_type)) => Self::Scalar {
                abi_type: *abi_type,
            },
            OutputShape::Handle { class_id, nullable } => Self::Handle {
                class_id: class_id.clone(),
                nullable: *nullable,
            },
            OutputShape::Callback {
                callback_id,
                nullable,
            } => Self::CallbackHandle {
                callback_id: callback_id.clone(),
                nullable: *nullable,
            },
            OutputShape::Value(
                ValueShape::OptionScalar { .. }
                | ValueShape::ResultScalar { .. }
                | ValueShape::PrimitiveVec { .. }
                | ValueShape::BlittableRecord { .. }
                | ValueShape::WireEncoded { .. },
            ) => {
                let decode_ops = output_shape_value_read_ops(result_shape);
                let encode_ops = output_shape_value_write_ops(result_shape);
                Self::WirePacket {
                    decode_ops,
                    encode_ops,
                }
            }
        }
    }
}

fn output_shape_value_read_ops(output_shape: &OutputShape) -> ReadSeq {
    match output_shape {
        OutputShape::Value(value_shape) => value_shape
            .read_ops()
            .unwrap_or_else(|| panic!("encoded output shape missing decode ops"))
            .clone(),
        _ => panic!("expected OutputShape::Value"),
    }
}

fn output_shape_value_write_ops(output_shape: &OutputShape) -> WriteSeq {
    match output_shape {
        OutputShape::Value(value_shape) => value_shape
            .write_ops()
            .unwrap_or_else(|| panic!("encoded output shape missing encode ops"))
            .clone(),
        _ => panic!("expected OutputShape::Value"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        AbiCall, AbiParam, AbiType, AsyncCall, CallId, CallMode, CallbackId, ClassId,
        ErrorTransport, FfiContract, InputShape, Lowerer, OutputShape, PackageInfo, ParamName,
        SizeExpr, TypeCatalog, ValueShape, WireShape,
    };
    use boltffi_ffi_rules::naming;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog: TypeCatalog::new(),
            functions: Vec::new(),
        }
    }

    #[test]
    fn transport_plan_builds_from_empty_abi_contract() {
        let abi = Lowerer::new(&empty_contract()).to_abi_contract();
        let plan = TransportPlan::from_abi(&abi);
        assert!(plan.calls.is_empty());
    }

    fn empty_read_seq() -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Fixed(0),
            ops: Vec::new(),
            shape: WireShape::Value,
        }
    }

    fn empty_write_seq() -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Fixed(0),
            ops: Vec::new(),
            shape: WireShape::Value,
        }
    }

    #[test]
    fn sync_input_abi_maps_supported_shapes() {
        let len_param = ParamName::new("payload_len");
        let class_id = ClassId::new("Connection");
        let callback_id = CallbackId::new("OnEvent");

        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::I32,
                input_shape: InputShape::Value(ValueShape::Scalar(AbiType::I32)),
            }),
            Some(SyncInputAbi::Scalar)
        ));
        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::Utf8Slice {
                    len_param: len_param.clone(),
                },
            }),
            Some(SyncInputAbi::Utf8Slice { .. })
        ));
        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::PrimitiveSlice {
                    len_param: len_param.clone(),
                    mutability: Mutability::Shared,
                    element_abi: AbiType::I32,
                },
            }),
            Some(SyncInputAbi::PrimitiveSlice { .. })
        ));
        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::WirePacket {
                    len_param: len_param.clone(),
                    value: ValueShape::WireEncoded {
                        read: empty_read_seq(),
                        write: empty_write_seq(),
                    },
                },
            }),
            Some(SyncInputAbi::WirePacket { .. })
        ));
        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::OutputBuffer {
                    len_param: len_param.clone(),
                    value: ValueShape::WireEncoded {
                        read: empty_read_seq(),
                        write: empty_write_seq(),
                    },
                },
            }),
            Some(SyncInputAbi::OutputBuffer { .. })
        ));
        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
            }),
            Some(SyncInputAbi::Handle { .. })
        ));
        assert!(matches!(
            SyncInputAbi::from_abi_param(&AbiParam {
                name: ParamName::new("value"),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::Callback {
                    callback_id: callback_id.clone(),
                    nullable: false,
                    style: CallbackStyle::BoxedDyn,
                },
            }),
            Some(SyncInputAbi::CallbackHandle { .. })
        ));
    }

    #[test]
    fn sync_input_abi_hides_hidden_shape() {
        let param = AbiParam {
            name: ParamName::new("value_len"),
            ffi_type: AbiType::U64,
            input_shape: InputShape::HiddenSyntheticLen {
                for_param: ParamName::new("value"),
            },
        };
        assert!(matches!(
            SyncParamAbi::from_abi_param(&param),
            SyncParamAbi::Hidden(SyncHiddenInputAbi::SyntheticLen { .. })
        ));
        assert!(matches!(
            SyncHiddenInputAbi::from_abi_param(&param),
            Some(SyncHiddenInputAbi::SyntheticLen { .. })
        ));
        assert!(SyncInputAbi::from_abi_param(&param).is_none());
    }

    #[test]
    fn output_abi_maps_sync_and_async_shapes() {
        let class_id = ClassId::new("Session");
        let callback_id = CallbackId::new("OnData");

        assert!(matches!(
            SyncOutputAbi::from_output_shape(&OutputShape::Unit),
            SyncOutputAbi::Unit
        ));
        assert!(matches!(
            SyncOutputAbi::from_output_shape(&OutputShape::Value(ValueShape::Scalar(AbiType::I64))),
            SyncOutputAbi::Scalar { .. }
        ));
        assert!(matches!(
            SyncOutputAbi::from_output_shape(&OutputShape::Value(ValueShape::WireEncoded {
                read: empty_read_seq(),
                write: empty_write_seq()
            })),
            SyncOutputAbi::WirePacket { .. }
        ));
        assert!(matches!(
            SyncOutputAbi::from_output_shape(&OutputShape::Handle {
                class_id: class_id.clone(),
                nullable: false
            }),
            SyncOutputAbi::Handle { .. }
        ));
        assert!(matches!(
            SyncOutputAbi::from_output_shape(&OutputShape::Callback {
                callback_id: callback_id.clone(),
                nullable: true
            }),
            SyncOutputAbi::CallbackHandle { .. }
        ));
        assert!(matches!(
            AsyncOutputAbi::from_result_shape(&OutputShape::Value(ValueShape::Scalar(
                AbiType::U32
            ))),
            AsyncOutputAbi::Scalar { .. }
        ));
        assert!(matches!(
            AsyncOutputAbi::from_result_shape(&OutputShape::Handle {
                class_id,
                nullable: true
            }),
            AsyncOutputAbi::Handle { .. }
        ));
        assert!(matches!(
            AsyncOutputAbi::from_result_shape(&OutputShape::Callback {
                callback_id,
                nullable: false
            }),
            AsyncOutputAbi::CallbackHandle { .. }
        ));
    }

    #[test]
    fn sync_input_abi_uses_input_shape_as_source_of_truth() {
        let param = AbiParam {
            name: ParamName::new("value"),
            ffi_type: AbiType::I32,
            input_shape: InputShape::HiddenOutDirect,
        };
        assert!(matches!(
            SyncParamAbi::from_abi_param(&param),
            SyncParamAbi::Hidden(SyncHiddenInputAbi::OutDirect)
        ));
        assert!(SyncInputAbi::from_abi_param(&param).is_none());

        let param = AbiParam {
            name: ParamName::new("value"),
            ffi_type: AbiType::I32,
            input_shape: InputShape::Value(ValueShape::Scalar(AbiType::I32)),
        };
        assert!(matches!(
            SyncInputAbi::from_abi_param(&param),
            Some(SyncInputAbi::Scalar)
        ));
    }

    #[test]
    fn sync_output_abi_prefers_output_shape_over_transport_kind() {
        let call = AbiCall {
            id: CallId::Function(crate::ir::FunctionId::new("sum")),
            symbol: naming::function_ffi_name("sum"),
            mode: CallMode::Sync,
            params: Vec::new(),
            output_shape: OutputShape::Value(ValueShape::Scalar(AbiType::I32)),
            error: ErrorTransport::None,
        };

        assert!(matches!(
            SyncOutputAbi::from_abi_call(&call),
            SyncOutputAbi::Scalar {
                abi_type: AbiType::I32
            }
        ));
    }

    #[test]
    fn async_output_abi_prefers_result_shape_over_transport_kind() {
        let async_call = AsyncCall {
            poll: naming::function_ffi_poll("sum_async"),
            complete: naming::function_ffi_complete("sum_async"),
            cancel: naming::function_ffi_cancel("sum_async"),
            free: naming::function_ffi_free("sum_async"),
            result_shape: OutputShape::Value(ValueShape::Scalar(AbiType::I64)),
            error: ErrorTransport::None,
        };

        assert!(matches!(
            AsyncOutputAbi::from_async_call(&async_call),
            AsyncOutputAbi::Scalar {
                abi_type: AbiType::I64
            }
        ));
    }

    #[test]
    fn render_backends_do_not_use_removed_route_constructors() {
        let swift_lower = include_str!("../render/swift/lower.rs");
        let kotlin_lower = include_str!("../render/kotlin/lower.rs");
        let typescript_lower = include_str!("../render/typescript/lower.rs");

        [swift_lower, kotlin_lower, typescript_lower]
            .into_iter()
            .for_each(|source| {
                assert!(!source.contains("from_param_role("));
                assert!(!source.contains("from_return_transport("));
                assert!(!source.contains("from_async_result_transport("));
                assert!(!source.contains("InputShape::"));
                assert!(!source.contains("OutputShape::"));
            });
    }
}
