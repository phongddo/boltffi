use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::naming;

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiParam, AbiStream, AsyncCall,
    CallId, CallMode, ErrorTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, EnumRepr,
    FunctionDef, MethodDef, ParamDef, Receiver, ReturnDef, StreamDef,
};
use crate::ir::ids::{CallbackId, EnumId, ParamName, RecordId};
use crate::ir::ops::SizeExpr;
use crate::ir::plan::{AbiType, Mutability, SpanContent};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::ir::{ParamRole, ReturnShape, ScalarOrigin, Transport};
use crate::render::kotlin::{NamingConvention, primitives};

use super::plan::{
    JniArrayReleaseMode, JniAsyncCallbackInvoker, JniAsyncCallbackMethod, JniAsyncCompleteKind,
    JniAsyncFunction, JniCallbackCParam, JniCallbackMethod, JniCallbackReturn, JniCallbackTrait,
    JniClass, JniClosureRecordParam, JniClosureTrampoline, JniClosureTrampolineReturn, JniFunction,
    JniInvokerResult, JniModule, JniOptionInnerKind, JniOptionView, JniParam, JniParamKind,
    JniPrimitiveArrayElementsKind, JniResultVariant, JniResultView, JniReturnKind, JniStream,
    JniWireCtor, JniWireFunction, JniWireMethod, TrampolineReturnStrategy,
};

struct JniReturnMeta {
    is_unit: bool,
    is_direct: bool,
    jni_return_type: String,
    jni_c_return_type: String,
    jni_result_cast: String,
}

/// Controls how JNI string parameters cross the FFI boundary.
///
/// `ByteArray` (default) passes strings as `jbyteArray` using
/// `GetByteArrayElements` which gives raw UTF-8 bytes with no
/// encoding conversion. The caller (Java/Kotlin) is responsible for
/// calling `String.getBytes(UTF_8)` / `toByteArray(Charsets.UTF_8)`
/// before the native call.
///
/// `JString` uses the classic `jstring` + `GetStringUTFChars` path
/// which returns Modified UTF-8 -- an encoding that mangles non-BMP
/// codepoints (emoji, some CJK) into CESU-8 surrogate pairs. This
/// silently corrupts any `char` above U+FFFF on round-trip.
///
/// `ByteArray` is both correct and safe:
///   - No encoding mismatch: Rust receives real UTF-8 bytes.
///   - Array length is O(1) via `GetArrayLength`; the `JString` path
///     requires an O(n) `strlen` after conversion.
///   - `GetByteArrayElements` does not enter a JNI critical region,
///     avoiding the strict GC/liveness constraints of
///     `GetPrimitiveArrayCritical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JniStringEncoding {
    JString,
    #[default]
    ByteArray,
}

pub struct JniLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    package: String,
    class_name: String,
    string_encoding: JniStringEncoding,
}

impl<'a> JniLowerer<'a> {
    pub fn new(
        contract: &'a FfiContract,
        abi: &'a AbiContract,
        package: String,
        class_name: String,
    ) -> Self {
        Self {
            contract,
            abi,
            package,
            class_name,
            string_encoding: JniStringEncoding::default(),
        }
    }

    pub fn with_string_encoding(mut self, encoding: JniStringEncoding) -> Self {
        self.string_encoding = encoding;
        self
    }

    pub fn lower(&self) -> JniModule {
        let prefix = naming::ffi_prefix().to_string();
        let jni_prefix = self.jni_prefix();
        let package_path = self.package.replace('.', "/");
        let module_name = self.contract.package.name.clone();
        let used_callbacks = self.collect_used_callbacks();

        let functions = self
            .contract
            .functions
            .iter()
            .filter(|func| !func.is_async && self.is_primitive_only(func))
            .map(|func| self.lower_function(func, &prefix, &jni_prefix))
            .collect();

        let wire_functions: Vec<JniWireFunction> = self
            .contract
            .functions
            .iter()
            .filter(|func| !func.is_async && !self.is_primitive_only(func))
            .map(|func| self.lower_wire_function(func, &jni_prefix))
            .chain(self.lower_value_type_wire_fns(&jni_prefix))
            .collect();

        let async_functions: Vec<JniAsyncFunction> = self
            .contract
            .functions
            .iter()
            .filter(|func| func.is_async)
            .map(|func| self.lower_async_function(func, &jni_prefix))
            .collect();

        let classes: Vec<JniClass> = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_class(class, &jni_prefix, &prefix))
            .collect();

        let callback_index = self
            .abi
            .callbacks
            .iter()
            .map(|callback| (callback.callback_id.clone(), callback))
            .collect::<HashMap<_, _>>();

        let callback_traits: Vec<JniCallbackTrait> = self
            .contract
            .catalog
            .all_callbacks()
            .filter(|callback| !matches!(callback.kind, CallbackKind::Closure))
            .filter(|callback| !callback.methods.is_empty())
            .filter_map(|callback| {
                callback_index.get(&callback.id).map(|abi_callback| {
                    self.lower_callback_trait(callback, abi_callback, &package_path, &jni_prefix)
                })
            })
            .collect();

        let has_async_callbacks = callback_traits
            .iter()
            .any(|callback| !callback.async_methods.is_empty());

        let async_callback_invokers = self.collect_async_invokers(&callback_traits, &jni_prefix);

        let closure_trampolines = self.collect_closure_trampolines(&package_path, &used_callbacks);

        let has_async = !async_functions.is_empty()
            || classes.iter().any(|class| !class.async_methods.is_empty())
            || classes.iter().any(|class| !class.streams.is_empty())
            || !callback_traits.is_empty();

        JniModule {
            prefix,
            jni_prefix,
            package_path,
            module_name,
            class_name: self.class_name.clone(),
            has_async,
            has_async_callbacks,
            functions,
            wire_functions,
            async_functions,
            classes,
            callback_traits,
            async_callback_invokers,
            closure_trampolines,
        }
    }

    fn jni_prefix(&self) -> String {
        self.package
            .replace('_', "_1")
            .replace('.', "_")
            .replace('-', "_1")
    }

    fn collect_used_callbacks(&self) -> HashSet<CallbackId> {
        let mut used = HashSet::new();

        self.abi
            .calls
            .iter()
            .for_each(|call| self.collect_used_from_call(call, &mut used));

        used
    }

    fn collect_used_from_call(&self, call: &AbiCall, used: &mut HashSet<CallbackId>) {
        call.params
            .iter()
            .for_each(|param| self.collect_used_from_param(param, used));
        self.collect_used_from_return(&call.returns, used);
        self.collect_used_from_error(&call.error, used);
        match &call.mode {
            CallMode::Sync => {}
            CallMode::Async(async_call) => self.collect_used_from_async(async_call.as_ref(), used),
        }
    }

    fn collect_used_from_param(&self, param: &AbiParam, used: &mut HashSet<CallbackId>) {
        if let ParamRole::Input {
            transport: Transport::Callback { callback_id, .. },
            ..
        } = &param.role
        {
            used.insert(callback_id.clone());
        }
    }

    fn collect_used_from_return(&self, returns: &ReturnShape, used: &mut HashSet<CallbackId>) {
        if let Some(Transport::Callback { callback_id, .. }) = &returns.transport {
            used.insert(callback_id.clone());
        }
    }

    fn collect_used_from_async(&self, async_call: &AsyncCall, used: &mut HashSet<CallbackId>) {
        if let Some(Transport::Callback { callback_id, .. }) = &async_call.result.transport {
            used.insert(callback_id.clone());
        }
        self.collect_used_from_error(&async_call.error, used);
    }

    fn collect_used_from_error(&self, _error: &ErrorTransport, _used: &mut HashSet<CallbackId>) {}

    fn is_primitive_only(&self, func: &FunctionDef) -> bool {
        if matches!(func.returns, ReturnDef::Result { .. }) {
            return false;
        }

        let abi_call = self.abi_call_for_function(func);

        let returns_ok = matches!(
            &abi_call.returns.transport,
            None | Some(Transport::Scalar(_))
        );

        let params_ok = abi_call.params.iter().all(|p| {
            matches!(
                p.role,
                ParamRole::Input {
                    transport: Transport::Scalar(_),
                    ..
                } | ParamRole::SyntheticLen { .. }
                    | ParamRole::CallbackContext { .. }
                    | ParamRole::OutLen { .. }
                    | ParamRole::OutDirect
                    | ParamRole::StatusOut
            )
        });

        returns_ok && params_ok
    }

    fn record_struct_size(&self, record_id: &RecordId) -> usize {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
            .and_then(|record| record.size)
            .unwrap_or(0)
    }

    fn lower_function(&self, func: &FunctionDef, prefix: &str, jni_prefix: &str) -> JniFunction {
        let ffi_name = format!("{}_{}", prefix, func.id.as_str());
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let abi_call = self.abi_call_for_function(func);
        let abi_inputs = self.input_abi_params(abi_call);
        let params: Vec<JniParam> = func
            .params
            .iter()
            .zip(abi_inputs.iter())
            .map(|(param, abi_param)| self.lower_param(param, abi_param))
            .collect();

        let return_kind = self.return_kind(&func.returns, func.id.as_str());

        let jni_return = self.return_kind_jni_return(&return_kind);
        let jni_params = self.format_jni_params(&params);

        JniFunction {
            ffi_name,
            jni_name,
            jni_return,
            jni_params,
            return_kind,
            params,
        }
    }

    fn lower_wire_function(&self, func: &FunctionDef, jni_prefix: &str) -> JniWireFunction {
        let ffi_name = naming::function_ffi_name(func.id.as_str()).into_string();
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let abi_call = self.abi_call_for_function(func);
        let abi_inputs = self.input_abi_params(abi_call);
        let params: Vec<JniParam> = func
            .params
            .iter()
            .zip(abi_inputs.iter())
            .map(|(param, abi_param)| self.lower_param(param, abi_param))
            .collect();

        let jni_params = self.format_jni_params(&params);
        let return_meta = self.return_meta(&func.returns);
        let return_composite_c_type = if matches!(func.returns, ReturnDef::Result { .. }) {
            None
        } else {
            self.composite_return_c_type(&abi_call.returns)
        };

        JniWireFunction {
            ffi_name,
            jni_name,
            jni_params,
            params,
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            return_composite_c_type,
            jni_return_type: return_meta.jni_return_type,
            jni_c_return_type: return_meta.jni_c_return_type,
            jni_result_cast: return_meta.jni_result_cast,
        }
    }

    fn lower_value_type_wire_fns(&self, jni_prefix: &str) -> Vec<JniWireFunction> {
        self.abi
            .calls
            .iter()
            .filter(|call| call.is_value_type_call())
            .filter(|call| matches!(call.mode, CallMode::Sync))
            .map(|call| {
                let param_defs = self.contract.catalog.params_for_value_call(&call.id);
                let abi_inputs = self.input_abi_params(call);
                let has_self = abi_inputs
                    .first()
                    .is_some_and(|p| p.name.as_str() == "self");
                let self_jni: Vec<JniParam> = if has_self {
                    vec![self.lower_value_self_param(abi_inputs[0])]
                } else {
                    vec![]
                };
                let skip = if has_self { 1 } else { 0 };
                let regular_jni: Vec<JniParam> = param_defs
                    .iter()
                    .zip(abi_inputs.iter().skip(skip))
                    .map(|(def, abi)| self.lower_param(def, abi))
                    .collect();
                let params: Vec<JniParam> = self_jni.into_iter().chain(regular_jni).collect();
                let jni_params = self.format_jni_params(&params);
                let return_meta = self.value_type_return_meta(call);
                let return_composite_c_type = self.composite_return_c_type(&call.returns);
                let ffi_name = call.symbol.as_str().to_string();
                let jni_name =
                    format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));
                JniWireFunction {
                    ffi_name,
                    jni_name,
                    jni_params,
                    params,
                    return_is_unit: return_meta.is_unit,
                    return_is_direct: return_meta.is_direct,
                    return_composite_c_type,
                    jni_return_type: return_meta.jni_return_type,
                    jni_c_return_type: return_meta.jni_c_return_type,
                    jni_result_cast: return_meta.jni_result_cast,
                }
            })
            .collect()
    }

    fn lower_value_self_param(&self, abi_param: &AbiParam) -> JniParam {
        let transport = match &abi_param.role {
            ParamRole::Input { transport, .. } => transport,
            _ => panic!("expected input role for self param"),
        };
        match transport {
            Transport::Scalar(origin) => {
                let prim = origin.primitive();
                let jni_type = self.primitive_return_jni_type(prim);
                let c_type = self.primitive_c_type(prim);
                JniParam {
                    name: "self_val".to_string(),
                    ffi_arg: format!("({})self_val", c_type),
                    jni_decl: format!(", {} self_val", jni_type),
                    kind: JniParamKind::Primitive,
                }
            }
            _ => JniParam {
                name: "self_buf".to_string(),
                ffi_arg: "(uint8_t*)_self_buf_ptr, (uintptr_t)_self_buf_len".to_string(),
                jni_decl: ", jobject self_buf".to_string(),
                kind: JniParamKind::Buffer,
            },
        }
    }

    fn value_type_return_meta(&self, abi_call: &AbiCall) -> JniReturnMeta {
        match &abi_call.returns.transport {
            None => JniReturnMeta {
                is_unit: true,
                is_direct: false,
                jni_return_type: "void".to_string(),
                jni_c_return_type: String::new(),
                jni_result_cast: String::new(),
            },
            Some(Transport::Scalar(origin)) => {
                let prim = origin.primitive();
                JniReturnMeta {
                    is_unit: false,
                    is_direct: true,
                    jni_return_type: self.primitive_return_jni_type(prim),
                    jni_c_return_type: self.primitive_c_type(prim),
                    jni_result_cast: self.primitive_return_cast(prim),
                }
            }
            _ => JniReturnMeta {
                is_unit: false,
                is_direct: false,
                jni_return_type: "jbyteArray".to_string(),
                jni_c_return_type: String::new(),
                jni_result_cast: String::new(),
            },
        }
    }

    fn lower_class(&self, class: &ClassDef, jni_prefix: &str, _prefix: &str) -> JniClass {
        let ffi_prefix = naming::class_ffi_prefix(class.id.as_str()).into_string();

        let ctors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, ctor)| self.lower_ctor(class, ctor, index, jni_prefix))
            .collect();

        let wire_methods = class
            .methods
            .iter()
            .filter(|method| !method.is_async)
            .map(|method| self.lower_method(class, method, jni_prefix))
            .collect();

        let async_methods = class
            .methods
            .iter()
            .filter(|method| method.is_async)
            .map(|method| self.lower_async_method(class, method, jni_prefix))
            .collect();
        let streams = class
            .streams
            .iter()
            .map(|stream| self.lower_stream(class, stream, jni_prefix))
            .collect();

        JniClass {
            ffi_prefix: ffi_prefix.clone(),
            jni_ffi_prefix: ffi_prefix.replace('_', "_1"),
            jni_prefix: jni_prefix.to_string(),
            ctors,
            wire_methods,
            async_methods,
            streams,
        }
    }

    fn lower_stream(&self, class: &ClassDef, stream: &StreamDef, jni_prefix: &str) -> JniStream {
        let abi_stream = self.abi_stream(class, stream);
        let subscribe_ffi = abi_stream.subscribe.as_str().to_string();
        let poll_ffi = abi_stream.poll.as_str().to_string();
        let pop_batch_ffi = abi_stream.pop_batch.as_str().to_string();
        let wait_ffi = abi_stream.wait.as_str().to_string();
        let unsubscribe_ffi = abi_stream.unsubscribe.as_str().to_string();
        let free_ffi = abi_stream.free.as_str().to_string();
        let subscribe_jni = format!(
            "Java_{}_Native_{}",
            jni_prefix,
            subscribe_ffi.replace('_', "_1")
        );
        let poll_jni = format!("Java_{}_Native_{}", jni_prefix, poll_ffi.replace('_', "_1"));
        let pop_batch_jni = format!(
            "Java_{}_Native_{}",
            jni_prefix,
            pop_batch_ffi.replace('_', "_1")
        );
        let wait_jni = format!("Java_{}_Native_{}", jni_prefix, wait_ffi.replace('_', "_1"));
        let unsubscribe_jni = format!(
            "Java_{}_Native_{}",
            jni_prefix,
            unsubscribe_ffi.replace('_', "_1")
        );
        let free_jni = format!("Java_{}_Native_{}", jni_prefix, free_ffi.replace('_', "_1"));
        JniStream {
            subscribe_ffi,
            subscribe_jni,
            poll_ffi,
            poll_jni,
            pop_batch_ffi,
            pop_batch_jni,
            wait_ffi,
            wait_jni,
            unsubscribe_ffi,
            unsubscribe_jni,
            free_ffi,
            free_jni,
        }
    }

    fn abi_stream<'b>(&'b self, class: &ClassDef, stream: &StreamDef) -> &'b AbiStream {
        self.abi
            .streams
            .iter()
            .find(|item| item.class_id == class.id && item.stream_id == stream.id)
            .expect("abi stream")
    }

    fn lower_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        jni_prefix: &str,
    ) -> JniWireMethod {
        let ffi_name = naming::method_ffi_name(class.id.as_str(), method.id.as_str()).into_string();
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let abi_call = self.abi_call_for_method(class, method);
        let abi_inputs = self.non_receiver_input_params(abi_call);
        let params: Vec<JniParam> = method
            .params
            .iter()
            .zip(abi_inputs.iter())
            .map(|(param, abi_param)| self.lower_param(param, abi_param))
            .collect();

        let jni_params = self.format_jni_params(&params);
        let return_meta = self.return_meta(&method.returns);
        let return_composite_c_type = if matches!(method.returns, ReturnDef::Result { .. }) {
            None
        } else {
            self.composite_return_c_type(&abi_call.returns)
        };

        JniWireMethod {
            ffi_name,
            jni_name,
            jni_params,
            params,
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            return_composite_c_type,
            jni_return_type: return_meta.jni_return_type,
            jni_c_return_type: return_meta.jni_c_return_type,
            jni_result_cast: return_meta.jni_result_cast,
            include_handle: !matches!(method.receiver, Receiver::Static),
        }
    }

    fn lower_ctor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        ctor_index: usize,
        jni_prefix: &str,
    ) -> JniWireCtor {
        let ffi_prefix = naming::class_ffi_prefix(class.id.as_str());
        let ffi_name = match ctor.name() {
            None => format!("{}_new", ffi_prefix),
            Some(name) => naming::method_ffi_name(class.id.as_str(), name.as_str()).into_string(),
        };

        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let abi_call = self.abi_call_for_constructor(class, ctor_index);
        let abi_inputs = self.input_abi_params(abi_call);
        let params: Vec<JniParam> = ctor
            .params()
            .into_iter()
            .zip(abi_inputs.iter())
            .map(|(param, abi_param)| self.lower_param(param, abi_param))
            .collect();

        let jni_params = self.format_jni_params(&params);

        JniWireCtor {
            ffi_name,
            jni_name,
            jni_params,
            params,
        }
    }

    fn lower_async_function(&self, func: &FunctionDef, jni_prefix: &str) -> JniAsyncFunction {
        let ffi_name = naming::function_ffi_name(func.id.as_str()).into_string();
        let jni_func_name = ffi_name.replace('_', "_1");

        let abi_call = self.abi_call_for_function(func);
        let abi_inputs = self.input_abi_params(abi_call);
        let params: Vec<JniParam> = func
            .params
            .iter()
            .zip(abi_inputs.iter())
            .map(|(param, abi_param)| self.lower_param(param, abi_param))
            .collect();

        let jni_params = self.format_jni_params(&params);

        let return_meta = self.return_meta(&func.returns);
        let complete_kind = self.async_complete_kind(&return_meta);

        JniAsyncFunction {
            ffi_name: ffi_name.clone(),
            ffi_poll: naming::function_ffi_poll(func.id.as_str()).into_string(),
            ffi_complete: naming::function_ffi_complete(func.id.as_str()).into_string(),
            ffi_cancel: naming::function_ffi_cancel(func.id.as_str()).into_string(),
            ffi_free: naming::function_ffi_free(func.id.as_str()).into_string(),
            jni_create_name: format!("Java_{}_Native_{}", jni_prefix, jni_func_name),
            jni_poll_name: format!("Java_{}_Native_{}_1poll", jni_prefix, jni_func_name),
            jni_complete_name: format!("Java_{}_Native_{}_1complete", jni_prefix, jni_func_name),
            jni_cancel_name: format!("Java_{}_Native_{}_1cancel", jni_prefix, jni_func_name),
            jni_free_name: format!("Java_{}_Native_{}_1free", jni_prefix, jni_func_name),
            jni_params,
            complete_kind,
            params,
        }
    }

    fn lower_async_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        jni_prefix: &str,
    ) -> JniAsyncFunction {
        let ffi_name = naming::method_ffi_name(class.id.as_str(), method.id.as_str()).into_string();
        let jni_func_name = ffi_name.replace('_', "_1");

        let abi_call = self.abi_call_for_method(class, method);
        let abi_inputs = self.non_receiver_input_params(abi_call);
        let params: Vec<JniParam> = method
            .params
            .iter()
            .zip(abi_inputs.iter())
            .map(|(param, abi_param)| self.lower_param(param, abi_param))
            .collect();

        let jni_params = self.format_jni_params(&params);

        let return_meta = self.return_meta(&method.returns);
        let complete_kind = self.async_complete_kind(&return_meta);

        JniAsyncFunction {
            ffi_name: ffi_name.clone(),
            ffi_poll: naming::method_ffi_poll(class.id.as_str(), method.id.as_str()).into_string(),
            ffi_complete: naming::method_ffi_complete(class.id.as_str(), method.id.as_str())
                .into_string(),
            ffi_cancel: naming::method_ffi_cancel(class.id.as_str(), method.id.as_str())
                .into_string(),
            ffi_free: naming::method_ffi_free(class.id.as_str(), method.id.as_str()).into_string(),
            jni_create_name: format!("Java_{}_Native_{}", jni_prefix, jni_func_name),
            jni_poll_name: format!("Java_{}_Native_{}_1poll", jni_prefix, jni_func_name),
            jni_complete_name: format!("Java_{}_Native_{}_1complete", jni_prefix, jni_func_name),
            jni_cancel_name: format!("Java_{}_Native_{}_1cancel", jni_prefix, jni_func_name),
            jni_free_name: format!("Java_{}_Native_{}_1free", jni_prefix, jni_func_name),
            jni_params,
            complete_kind,
            params,
        }
    }

    fn lower_param(&self, param: &ParamDef, abi_param: &AbiParam) -> JniParam {
        let name = naming::escape_c_keyword(param.name.as_str());

        let transport = match &abi_param.role {
            ParamRole::Input { transport, .. } => transport,
            _ => unreachable!("lower_param called with non-input AbiParam"),
        };

        let mutability = match &abi_param.role {
            ParamRole::Input { mutability, .. } => *mutability,
            _ => Mutability::Shared,
        };

        let (jni_type, ffi_arg, kind) = match transport {
            Transport::Scalar(_) => {
                let jni_type = self.scalar_jni_type(&abi_param.abi_type);
                let ffi_arg = name.clone();
                (jni_type, ffi_arg, JniParamKind::Primitive)
            }
            Transport::Span(SpanContent::Utf8) => {
                if self.string_encoding == JniStringEncoding::ByteArray {
                    let jni_type = "jbyteArray".to_string();
                    let ffi_arg =
                        format!("(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len", name, name);
                    let kind = JniParamKind::PrimitiveArray {
                        c_type: "uint8_t".to_string(),
                        elements_kind: JniPrimitiveArrayElementsKind::Byte,
                        release_mode: JniArrayReleaseMode::Abort,
                    };
                    (jni_type, ffi_arg, kind)
                } else {
                    let jni_type = "jstring".to_string();
                    let ffi_arg = format!(
                        "(const uint8_t*)_{}_c, (_{}_c != NULL) ? strlen(_{}_c) : 0",
                        name, name, name
                    );
                    (jni_type, ffi_arg, JniParamKind::String)
                }
            }
            Transport::Span(SpanContent::Scalar(origin)) => {
                if self.use_buffer_for_span_scalar_param(param, origin) {
                    let jni_type = "jobject".to_string();
                    let ptr_type = match origin {
                        ScalarOrigin::Primitive(PrimitiveType::ISize) => "const intptr_t*",
                        ScalarOrigin::Primitive(PrimitiveType::USize) => "const uintptr_t*",
                        _ => {
                            unreachable!("buffer span scalar override only applies to isize/usize")
                        }
                    };
                    let ffi_arg = format!("({})_{}_ptr, (uintptr_t)_{}_len", ptr_type, name, name);
                    return JniParam {
                        name,
                        ffi_arg,
                        jni_decl: format!(
                            "{} {}",
                            jni_type,
                            naming::escape_c_keyword(param.name.as_str())
                        ),
                        kind: JniParamKind::Buffer,
                    };
                }
                let primitive = origin.primitive();
                let c_type = self.primitive_c_type(primitive);
                let is_mutable = matches!(mutability, Mutability::Mutable);
                let ptr_type = if is_mutable {
                    format!("{}*", c_type)
                } else {
                    format!("const {}*", c_type)
                };
                let ffi_arg = format!("({})_{}_ptr, (uintptr_t)_{}_len", ptr_type, name, name);
                let jni_type = self.primitive_array_jni_type(primitive);
                let release_mode = if is_mutable {
                    JniArrayReleaseMode::Commit
                } else {
                    JniArrayReleaseMode::Abort
                };
                let elements_kind = self.primitive_array_elements_kind(primitive);
                let kind = JniParamKind::PrimitiveArray {
                    c_type,
                    elements_kind,
                    release_mode,
                };
                (jni_type, ffi_arg, kind)
            }
            Transport::Composite(layout) => {
                let c_type = format!("___{}", layout.record_id.as_str());
                let jni_type = "jobject".to_string();
                let ffi_arg = format!("_{}_val", name);
                let kind = JniParamKind::Composite { c_type };
                (jni_type, ffi_arg, kind)
            }
            Transport::Span(SpanContent::Encoded(_))
            | Transport::Span(SpanContent::Composite(_)) => {
                let jni_type = "jobject".to_string();
                let ffi_arg = format!("(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len", name, name);
                (jni_type, ffi_arg, JniParamKind::Buffer)
            }
            Transport::Handle { .. } => {
                let jni_type = "jlong".to_string();
                let ffi_arg = format!("(void*){}", name);
                (jni_type, ffi_arg, JniParamKind::Primitive)
            }
            Transport::Callback {
                callback_id,
                nullable: _,
                style: _,
            } => {
                let is_closure = self.is_closure_callback(callback_id);
                let jni_type = "jlong".to_string();
                let ffi_arg = if is_closure {
                    let trampoline = self.closure_trampoline_name(callback_id);
                    format!("{}, (void*){}", trampoline, name)
                } else {
                    let create_fn = naming::callback_create_fn(callback_id.as_str()).into_string();
                    format!("{}((uint64_t){})", create_fn, name)
                };
                let kind = if is_closure {
                    JniParamKind::Closure
                } else {
                    JniParamKind::Primitive
                };
                (jni_type, ffi_arg, kind)
            }
        };

        let jni_decl = format!("{} {}", jni_type, name);

        JniParam {
            name,
            ffi_arg,
            jni_decl,
            kind,
        }
    }

    fn use_buffer_for_span_scalar_param(&self, param: &ParamDef, origin: &ScalarOrigin) -> bool {
        matches!(
            (&param.type_expr, origin),
            (
                TypeExpr::Vec(inner),
                ScalarOrigin::Primitive(PrimitiveType::ISize | PrimitiveType::USize)
            ) if matches!(inner.as_ref(), TypeExpr::Primitive(PrimitiveType::ISize | PrimitiveType::USize))
        )
    }

    fn scalar_jni_type(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "jboolean".to_string(),
            AbiType::I8 | AbiType::U8 => "jbyte".to_string(),
            AbiType::I16 | AbiType::U16 => "jshort".to_string(),
            AbiType::I32 | AbiType::U32 => "jint".to_string(),
            AbiType::I64 | AbiType::U64 | AbiType::ISize | AbiType::USize => "jlong".to_string(),
            AbiType::F32 => "jfloat".to_string(),
            AbiType::F64 => "jdouble".to_string(),
            AbiType::Pointer(_)
            | AbiType::InlineCallbackFn { .. }
            | AbiType::Handle(_)
            | AbiType::CallbackHandle
            | AbiType::Struct(_)
            | AbiType::Void => "jlong".to_string(),
        }
    }

    fn primitive_array_jni_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::I8 | PrimitiveType::U8 => "jbyteArray",
            PrimitiveType::I16 | PrimitiveType::U16 => "jshortArray",
            PrimitiveType::I32 | PrimitiveType::U32 => "jintArray",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "jlongArray",
            PrimitiveType::F32 => "jfloatArray",
            PrimitiveType::F64 => "jdoubleArray",
            PrimitiveType::Bool => "jbooleanArray",
        }
        .to_string()
    }

    fn param_jni_type(
        &self,
        ty: &TypeExpr,
        is_wire_param: bool,
        is_data_enum: bool,
        is_array: bool,
        is_closure: bool,
    ) -> String {
        if is_closure {
            return "jlong".to_string();
        }

        if is_data_enum || is_wire_param {
            return "jobject".to_string();
        }

        if is_array {
            return self.array_jni_type(ty).to_string();
        }

        match ty {
            TypeExpr::Primitive(p) => self.primitive_jni_type(*p).to_string(),
            TypeExpr::String => "jstring".to_string(),
            TypeExpr::Bytes => "jbyteArray".to_string(),
            TypeExpr::Handle(_) | TypeExpr::Callback(_) => "jlong".to_string(),
            TypeExpr::Enum(_) => "jint".to_string(),
            _ => "jlong".to_string(),
        }
    }

    fn array_jni_type(&self, ty: &TypeExpr) -> &str {
        match ty {
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(PrimitiveType::I8 | PrimitiveType::U8) => "jbyteArray",
                TypeExpr::Primitive(PrimitiveType::I16 | PrimitiveType::U16) => "jshortArray",
                TypeExpr::Primitive(PrimitiveType::I32 | PrimitiveType::U32) => "jintArray",
                TypeExpr::Primitive(
                    PrimitiveType::I64
                    | PrimitiveType::U64
                    | PrimitiveType::ISize
                    | PrimitiveType::USize,
                ) => "jlongArray",
                TypeExpr::Primitive(PrimitiveType::F32) => "jfloatArray",
                TypeExpr::Primitive(PrimitiveType::F64) => "jdoubleArray",
                TypeExpr::Primitive(PrimitiveType::Bool) => "jbooleanArray",
                _ => "jobject",
            },
            TypeExpr::Bytes => "jbyteArray",
            _ => "jobject",
        }
    }

    fn primitive_c_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::I8 => "int8_t".to_string(),
            PrimitiveType::U8 => "uint8_t".to_string(),
            PrimitiveType::I16 => "int16_t".to_string(),
            PrimitiveType::U16 => "uint16_t".to_string(),
            PrimitiveType::I32 => "int32_t".to_string(),
            PrimitiveType::U32 => "uint32_t".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "int64_t".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "uint64_t".to_string(),
            PrimitiveType::F32 => "float".to_string(),
            PrimitiveType::F64 => "double".to_string(),
        }
    }

    fn primitive_array_elements_kind(
        &self,
        primitive: PrimitiveType,
    ) -> JniPrimitiveArrayElementsKind {
        match primitive {
            PrimitiveType::Bool => JniPrimitiveArrayElementsKind::Boolean,
            PrimitiveType::I8 | PrimitiveType::U8 => JniPrimitiveArrayElementsKind::Byte,
            PrimitiveType::I16 | PrimitiveType::U16 => JniPrimitiveArrayElementsKind::Short,
            PrimitiveType::I32 | PrimitiveType::U32 => JniPrimitiveArrayElementsKind::Int,
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => JniPrimitiveArrayElementsKind::Long,
            PrimitiveType::F32 => JniPrimitiveArrayElementsKind::Float,
            PrimitiveType::F64 => JniPrimitiveArrayElementsKind::Double,
        }
    }

    fn primitive_jni_type(&self, primitive: PrimitiveType) -> &'static str {
        let model_primitive = primitive;
        primitives::info(model_primitive).jni_type
    }

    fn primitive_signature(&self, primitive: PrimitiveType) -> String {
        let model_primitive = primitive;
        primitives::info(model_primitive).signature.to_string()
    }

    fn composite_return_c_type(&self, returns: &ReturnShape) -> Option<String> {
        match &returns.transport {
            Some(Transport::Composite(layout)) => Some(format!("___{}", layout.record_id.as_str())),
            _ => None,
        }
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| call.id == CallId::Function(func.id.clone()))
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

    fn input_abi_params<'b>(&self, call: &'b AbiCall) -> Vec<&'b AbiParam> {
        call.params
            .iter()
            .filter(|p| matches!(p.role, ParamRole::Input { .. }))
            .collect()
    }

    fn non_receiver_input_params<'b>(&self, call: &'b AbiCall) -> Vec<&'b AbiParam> {
        call.params
            .iter()
            .filter(|p| {
                matches!(p.role, ParamRole::Input { .. })
                    && !(p.name.as_str() == "self"
                        && matches!(
                            p.role,
                            ParamRole::Input {
                                transport: Transport::Handle { .. },
                                ..
                            }
                        ))
            })
            .collect()
    }

    fn needs_wire_encoding(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Builtin(_)
            | TypeExpr::Record(_)
            | TypeExpr::Enum(_)
            | TypeExpr::Custom(_) => true,
            TypeExpr::Vec(inner) => !matches!(inner.as_ref(), TypeExpr::Primitive(_)),
            TypeExpr::Option(inner) => {
                !matches!(inner.as_ref(), TypeExpr::Handle(_) | TypeExpr::Callback(_))
            }
            _ => false,
        }
    }

    fn record_param_info(&self, ty: &TypeExpr) -> Option<RecordParamInfo> {
        match ty {
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Record(id) => {
                    let struct_size = self.record_struct_size(id);
                    Some(RecordParamInfo {
                        id: id.clone(),
                        struct_size,
                    })
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn data_enum_param_info(&self, ty: &TypeExpr) -> Option<DataEnumParamInfo> {
        match ty {
            TypeExpr::Enum(id) => self
                .contract
                .catalog
                .resolve_enum(id)
                .filter(|enum_def| matches!(enum_def.repr, EnumRepr::Data { .. }))
                .map(|_| DataEnumParamInfo { id: id.clone() }),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn param_ffi_arg(
        &self,
        name: &str,
        ty: &TypeExpr,
        array_primitive: Option<PrimitiveType>,
        array_is_mutable: bool,
        is_wire_param: bool,
        record_info: Option<RecordParamInfo>,
        data_enum_info: Option<DataEnumParamInfo>,
    ) -> String {
        if matches!(ty, TypeExpr::String) {
            return format!(
                "(const uint8_t*)_{}_c, (_{}_c != NULL) ? strlen(_{}_c) : 0",
                name, name, name
            );
        }

        if record_info.is_some() || data_enum_info.is_some() {
            return format!("(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len", name, name);
        }

        if let Some(primitive) = array_primitive {
            let c_type = self.primitive_c_type(primitive);
            let ptr_type = if array_is_mutable {
                format!("{}*", c_type)
            } else {
                format!("const {}*", c_type)
            };
            return format!("({})_{}_ptr, (uintptr_t)_{}_len", ptr_type, name, name);
        }

        if is_wire_param {
            return format!("(const uint8_t*)_{}_ptr, (uintptr_t)_{}_len", name, name);
        }

        match ty {
            TypeExpr::Handle(_) => format!("(void*){}", name),
            TypeExpr::Callback(callback_id) => {
                if self.is_closure_callback(callback_id) {
                    let trampoline = self.closure_trampoline_name(callback_id);
                    format!("{}, (void*){}", trampoline, name)
                } else {
                    let create_fn = naming::callback_create_fn(callback_id.as_str()).into_string();
                    format!("{}((uint64_t){})", create_fn, name)
                }
            }
            _ => name.to_string(),
        }
    }

    fn is_closure_callback(&self, callback_id: &CallbackId) -> bool {
        self.contract
            .catalog
            .resolve_callback(callback_id)
            .map(|callback| matches!(callback.kind, CallbackKind::Closure))
            .unwrap_or(false)
    }

    fn closure_trampoline_name(&self, callback_id: &CallbackId) -> String {
        let signature_id = callback_id
            .as_str()
            .strip_prefix("__Closure_")
            .unwrap_or(callback_id.as_str());
        format!("trampoline_{}", signature_id)
    }

    fn format_jni_params(&self, params: &[JniParam]) -> String {
        if params.is_empty() {
            String::new()
        } else {
            let decls = params
                .iter()
                .map(|param| param.jni_param_decl().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(", {}", decls)
        }
    }

    fn return_kind(&self, returns: &ReturnDef, func_name: &str) -> JniReturnKind {
        match returns {
            ReturnDef::Void => JniReturnKind::Void,
            ReturnDef::Result { ok, err } => {
                JniReturnKind::Result(self.result_view(ok, err, func_name))
            }
            ReturnDef::Value(ty) => self.return_kind_from_type(ty, func_name),
        }
    }

    fn return_kind_from_type(&self, ty: &TypeExpr, func_name: &str) -> JniReturnKind {
        match ty {
            TypeExpr::Void => JniReturnKind::Void,
            TypeExpr::Primitive(p) => JniReturnKind::Primitive {
                jni_type: self.primitive_return_jni_type(*p),
            },
            TypeExpr::String => JniReturnKind::String {
                ffi_name: naming::function_ffi_name(func_name).into_string(),
            },
            TypeExpr::Vec(_) => JniReturnKind::Vec {
                len_fn: naming::function_ffi_vec_len(func_name).into_string(),
                copy_fn: naming::function_ffi_vec_copy_into(func_name).into_string(),
            },
            TypeExpr::Enum(id) => self
                .contract
                .catalog
                .resolve_enum(id)
                .filter(|enum_def| matches!(enum_def.repr, EnumRepr::Data { .. }))
                .map(|_| {
                    let struct_size = self.data_enum_struct_size(id);
                    let enum_name = NamingConvention::class_name(id.as_str());
                    JniReturnKind::DataEnum {
                        enum_name,
                        struct_size,
                    }
                })
                .unwrap_or_else(|| {
                    let tag_type = self
                        .contract
                        .catalog
                        .resolve_enum(id)
                        .and_then(|enum_def| match enum_def.repr {
                            EnumRepr::CStyle { tag_type, .. } => Some(tag_type),
                            _ => None,
                        })
                        .unwrap_or(PrimitiveType::I32);
                    JniReturnKind::CStyleEnum {
                        jni_type: self.primitive_return_jni_type(tag_type),
                    }
                }),
            TypeExpr::Option(inner) => {
                let opt = self.option_view(inner);
                JniReturnKind::Option(opt)
            }
            _ => JniReturnKind::Void,
        }
    }

    fn return_kind_jni_return(&self, kind: &JniReturnKind) -> String {
        match kind {
            JniReturnKind::Void => "void".to_string(),
            JniReturnKind::Primitive { jni_type } => jni_type.clone(),
            JniReturnKind::String { .. } => "jstring".to_string(),
            JniReturnKind::Vec { .. } => "jobject".to_string(),
            JniReturnKind::CStyleEnum { jni_type } => jni_type.clone(),
            JniReturnKind::DataEnum { .. } => "jobject".to_string(),
            JniReturnKind::Option(_) => "jobject".to_string(),
            JniReturnKind::Result(_) => "jobject".to_string(),
        }
    }

    fn return_meta(&self, returns: &ReturnDef) -> JniReturnMeta {
        match returns {
            ReturnDef::Void => JniReturnMeta {
                is_unit: true,
                is_direct: false,
                jni_return_type: "void".to_string(),
                jni_c_return_type: String::new(),
                jni_result_cast: String::new(),
            },
            ReturnDef::Result { .. } => JniReturnMeta {
                is_unit: false,
                is_direct: false,
                jni_return_type: "jbyteArray".to_string(),
                jni_c_return_type: String::new(),
                jni_result_cast: String::new(),
            },
            ReturnDef::Value(ty) => match ty {
                TypeExpr::Void => JniReturnMeta {
                    is_unit: true,
                    is_direct: false,
                    jni_return_type: "void".to_string(),
                    jni_c_return_type: String::new(),
                    jni_result_cast: String::new(),
                },
                TypeExpr::Primitive(p) => JniReturnMeta {
                    is_unit: false,
                    is_direct: true,
                    jni_return_type: self.primitive_return_jni_type(*p),
                    jni_c_return_type: self.primitive_c_type(*p),
                    jni_result_cast: self.primitive_return_cast(*p),
                },
                TypeExpr::Enum(id)
                    if self
                        .contract
                        .catalog
                        .resolve_enum(id)
                        .is_some_and(|e| !matches!(e.repr, EnumRepr::Data { .. })) =>
                {
                    let tag_type = self
                        .contract
                        .catalog
                        .resolve_enum(id)
                        .and_then(|e| match e.repr {
                            EnumRepr::CStyle { tag_type, .. } => Some(tag_type),
                            _ => None,
                        })
                        .unwrap_or(PrimitiveType::I32);
                    JniReturnMeta {
                        is_unit: false,
                        is_direct: true,
                        jni_return_type: self.primitive_return_jni_type(tag_type),
                        jni_c_return_type: self.primitive_c_type(tag_type),
                        jni_result_cast: self.primitive_return_cast(tag_type),
                    }
                }
                TypeExpr::String
                | TypeExpr::Record(_)
                | TypeExpr::Enum(_)
                | TypeExpr::Vec(_)
                | TypeExpr::Option(_)
                | TypeExpr::Bytes
                | TypeExpr::Builtin(_)
                | TypeExpr::Custom(_) => JniReturnMeta {
                    is_unit: false,
                    is_direct: false,
                    jni_return_type: "jbyteArray".to_string(),
                    jni_c_return_type: String::new(),
                    jni_result_cast: String::new(),
                },
                _ => JniReturnMeta {
                    is_unit: false,
                    is_direct: true,
                    jni_return_type: "jlong".to_string(),
                    jni_c_return_type: "int64_t".to_string(),
                    jni_result_cast: "".to_string(),
                },
            },
        }
    }

    fn async_complete_kind(&self, return_meta: &JniReturnMeta) -> JniAsyncCompleteKind {
        if return_meta.is_unit {
            JniAsyncCompleteKind::Void
        } else if return_meta.is_direct {
            JniAsyncCompleteKind::Direct {
                jni_return: return_meta.jni_return_type.clone(),
                c_type: return_meta.jni_c_return_type.clone(),
            }
        } else {
            JniAsyncCompleteKind::WireEncoded
        }
    }

    fn primitive_return_jni_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "jboolean".to_string(),
            PrimitiveType::I8 | PrimitiveType::U8 => "jbyte".to_string(),
            PrimitiveType::I16 | PrimitiveType::U16 => "jshort".to_string(),
            PrimitiveType::I32 | PrimitiveType::U32 => "jint".to_string(),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "jlong".to_string(),
            PrimitiveType::F32 => "jfloat".to_string(),
            PrimitiveType::F64 => "jdouble".to_string(),
        }
    }

    fn primitive_return_cast(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "(jboolean)".to_string(),
            PrimitiveType::U8 => "(jbyte)".to_string(),
            PrimitiveType::U16 => "(jshort)".to_string(),
            PrimitiveType::U32 => "(jint)".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "(jlong)".to_string(),
            _ => String::new(),
        }
    }

    fn result_view(&self, ok: &TypeExpr, err: &TypeExpr, func_name: &str) -> JniResultView {
        let len_fn = naming::function_ffi_vec_len(func_name).into_string();
        let copy_fn = naming::function_ffi_vec_copy_into(func_name).into_string();

        let ok_variant = self.result_variant(ok, &len_fn, &copy_fn);
        let err_variant = self.result_variant(err, &len_fn, &copy_fn);

        JniResultView {
            ok: ok_variant,
            err: err_variant,
        }
    }

    fn result_variant(&self, ty: &TypeExpr, len_fn: &str, copy_fn: &str) -> JniResultVariant {
        match ty {
            TypeExpr::Void => JniResultVariant::Void,
            TypeExpr::Primitive(p) => JniResultVariant::Primitive {
                c_type: self.primitive_c_type(*p),
                jni_type: self.primitive_return_jni_type(*p),
            },
            TypeExpr::String => JniResultVariant::String,
            TypeExpr::Record(id) => JniResultVariant::Record {
                c_type: NamingConvention::class_name(id.as_str()),
                jni_type: "jobject".to_string(),
                struct_size: self.record_struct_size(id),
            },
            TypeExpr::Enum(id) => {
                let enum_def = self.contract.catalog.resolve_enum(id);
                let is_data_enum = enum_def
                    .as_ref()
                    .map(|def| matches!(def.repr, EnumRepr::Data { .. }) || def.is_error)
                    .unwrap_or(false);
                if is_data_enum {
                    JniResultVariant::DataEnum {
                        jni_type: "jobject".to_string(),
                        struct_size: self.data_enum_struct_size(id),
                    }
                } else {
                    JniResultVariant::Enum {
                        jni_type: "jint".to_string(),
                    }
                }
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => JniResultVariant::VecPrimitive {
                    info: self.vec_primitive_info(*p),
                    len_fn: len_fn.to_string(),
                    copy_fn: copy_fn.to_string(),
                },
                TypeExpr::Record(id) => JniResultVariant::VecRecord {
                    len_fn: len_fn.to_string(),
                    copy_fn: copy_fn.to_string(),
                    struct_size: self.record_struct_size(id),
                },
                _ => JniResultVariant::Void,
            },
            _ => JniResultVariant::Void,
        }
    }

    fn vec_primitive_info(&self, primitive: PrimitiveType) -> super::plan::JniVecPrimitive {
        let model_primitive = primitive;
        let info = primitives::info(model_primitive);
        super::plan::JniVecPrimitive {
            c_type_name: info.c_type.to_string(),
            jni_array_type: info.array_type.to_string(),
        }
    }

    fn option_view(&self, inner: &TypeExpr) -> JniOptionView {
        let is_vec = matches!(inner, TypeExpr::Vec(_));
        let is_data_enum = self.is_data_enum(inner);
        let struct_size = match inner {
            TypeExpr::Record(id) => self.record_struct_size(id),
            TypeExpr::Enum(id) if is_data_enum => self.data_enum_struct_size(id),
            TypeExpr::Vec(vec_inner) => match vec_inner.as_ref() {
                TypeExpr::Record(id) => self.record_struct_size(id),
                TypeExpr::Enum(id) if is_data_enum => self.data_enum_struct_size(id),
                _ => 0,
            },
            _ => 0,
        };

        let inner_kind = self.option_inner_kind(inner, is_data_enum);

        JniOptionView {
            ffi_type: self.option_ffi_type(inner, is_vec, is_data_enum),
            struct_size,
            inner_kind,
        }
    }

    fn option_inner_kind(&self, inner: &TypeExpr, is_data_enum: bool) -> JniOptionInnerKind {
        match inner {
            TypeExpr::Primitive(p) => {
                if self.primitive_is_large(*p) {
                    JniOptionInnerKind::PrimitiveLarge
                } else {
                    JniOptionInnerKind::Primitive32
                }
            }
            TypeExpr::String => JniOptionInnerKind::String,
            TypeExpr::Record(_) => JniOptionInnerKind::Record,
            TypeExpr::Enum(_) if is_data_enum => JniOptionInnerKind::Enum,
            TypeExpr::Enum(_) => JniOptionInnerKind::Enum,
            TypeExpr::Vec(vec_inner) => match vec_inner.as_ref() {
                TypeExpr::Primitive(_) => JniOptionInnerKind::VecPrimitive,
                TypeExpr::Record(_) => JniOptionInnerKind::VecRecord,
                TypeExpr::String => JniOptionInnerKind::VecString,
                TypeExpr::Enum(_) => JniOptionInnerKind::VecEnum,
                _ => JniOptionInnerKind::VecPrimitive,
            },
            _ => JniOptionInnerKind::Record,
        }
    }

    fn option_ffi_type(&self, inner: &TypeExpr, is_vec: bool, is_data_enum: bool) -> String {
        if is_vec {
            match inner {
                TypeExpr::Vec(vec_inner) => match vec_inner.as_ref() {
                    TypeExpr::Primitive(p) => format!("FfiOption_{}", self.primitive_c_type(*p)),
                    TypeExpr::Record(id) => {
                        format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                    }
                    TypeExpr::String => "FfiOption_FfiString".to_string(),
                    TypeExpr::Enum(id) if is_data_enum => {
                        format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                    }
                    TypeExpr::Enum(_) => "FfiOption_int32_t".to_string(),
                    _ => "FfiOption_void".to_string(),
                },
                _ => "FfiOption_void".to_string(),
            }
        } else {
            match inner {
                TypeExpr::Primitive(p) => format!("FfiOption_{}", self.primitive_c_type(*p)),
                TypeExpr::String => "FfiOption_FfiString".to_string(),
                TypeExpr::Record(id) => {
                    format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                }
                TypeExpr::Enum(id) if is_data_enum => {
                    format!("FfiOption_{}", NamingConvention::class_name(id.as_str()))
                }
                TypeExpr::Enum(_) => "FfiOption_int32_t".to_string(),
                _ => "FfiOption_void".to_string(),
            }
        }
    }

    fn primitive_is_large(&self, primitive: PrimitiveType) -> bool {
        matches!(
            primitive,
            PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize
                | PrimitiveType::F64
        )
    }

    fn is_data_enum(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Enum(id) => self
                .contract
                .catalog
                .resolve_enum(id)
                .map(|enum_def| matches!(enum_def.repr, EnumRepr::Data { .. }))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn fixed_size(&self, size: &SizeExpr) -> Option<usize> {
        match size {
            SizeExpr::Fixed(value) => Some(*value),
            SizeExpr::Sum(parts) => parts
                .iter()
                .map(|part| self.fixed_size(part))
                .collect::<Option<Vec<_>>>()
                .map(|sizes| sizes.into_iter().sum()),
            _ => None,
        }
    }

    fn data_enum_struct_size(&self, enum_id: &EnumId) -> usize {
        self.abi
            .enums
            .iter()
            .find(|enum_def| enum_def.id == *enum_id)
            .and_then(|enum_def| self.fixed_size(&enum_def.encode_ops.size))
            .unwrap_or(0)
    }

    fn lower_callback_trait(
        &self,
        callback: &CallbackTraitDef,
        abi_callback: &AbiCallbackInvocation,
        package_path: &str,
        jni_prefix: &str,
    ) -> JniCallbackTrait {
        let trait_name = NamingConvention::class_name(callback.id.as_str());
        let callbacks_class = format!("{}Callbacks", trait_name);
        let abi_methods: HashMap<_, _> = abi_callback
            .methods
            .iter()
            .map(|method| (method.id.clone(), method))
            .collect();

        let sync_methods = callback
            .methods
            .iter()
            .filter(|method| !method.is_async)
            .filter(|method| self.callback_method_supported(callback, method))
            .filter_map(|method| {
                let abi_method = abi_methods.get(&method.id)?;
                Some(self.lower_sync_callback_method(method, abi_method))
            })
            .collect();

        let async_methods = callback
            .methods
            .iter()
            .filter(|method| method.is_async)
            .filter(|method| self.callback_method_supported(callback, method))
            .filter_map(|method| {
                let abi_method = abi_methods.get(&method.id)?;
                Some(self.lower_async_callback_method(method, abi_method, jni_prefix))
            })
            .collect();

        JniCallbackTrait {
            trait_name,
            vtable_type: abi_callback.vtable_type.as_str().to_string(),
            register_fn: abi_callback.register_fn.as_str().to_string(),
            callbacks_class: format!("{}/{}", package_path, callbacks_class),
            sync_methods,
            async_methods,
        }
    }

    fn callback_method_supported(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> bool {
        let reasons = self.callback_method_unsupported_reasons(method);
        if reasons.is_empty() {
            true
        } else {
            reasons.iter().for_each(|reason| {
                eprintln!(
                    "[boltffi][jni] skipping callback method `{}.{}`: {}",
                    callback.id.as_str(),
                    method.id.as_str(),
                    reason
                )
            });
            false
        }
    }

    fn callback_method_unsupported_reasons(&self, method: &CallbackMethodDef) -> Vec<String> {
        let param_reasons = method.params.iter().filter_map(|param| {
            self.unsupported_callback_param_reason(&param.type_expr)
                .map(|reason| format!("parameter `{}` ({})", param.name.as_str(), reason))
        });

        param_reasons.collect()
    }

    fn unsupported_callback_param_reason(&self, ty: &TypeExpr) -> Option<String> {
        match ty {
            TypeExpr::Handle(_) => Some("Handle not supported in callback params".to_string()),
            TypeExpr::Callback(_) => Some("Callback not supported in callback params".to_string()),
            _ => None,
        }
    }

    fn lower_sync_callback_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JniCallbackMethod {
        let callback_out_params: Vec<&AbiParam> = abi_method
            .params
            .iter()
            .filter(|param| matches!(&param.role, ParamRole::OutDirect | ParamRole::OutLen { .. }))
            .collect();

        let out_direct_param = callback_out_params
            .iter()
            .find_map(|param| matches!(&param.role, ParamRole::OutDirect).then_some(*param));

        let out_len_param = callback_out_params
            .iter()
            .find_map(|param| matches!(&param.role, ParamRole::OutLen { .. }).then_some(*param));

        let return_info =
            self.sync_callback_return_info(&abi_method.returns, out_direct_param, out_len_param);

        let lowered_params: Vec<LoweredCallbackParam> = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(&param.name, &param.type_expr, false))
            .collect();

        let input_c_params = lowered_params
            .iter()
            .flat_map(|param| param.c_params.iter().cloned())
            .collect::<Vec<_>>();

        let out_c_params = callback_out_params
            .iter()
            .filter_map(|param| self.callback_out_param(param))
            .collect::<Vec<_>>();

        let c_params = input_c_params.into_iter().chain(out_c_params).collect();

        let setup_lines = lowered_params
            .iter()
            .flat_map(|param| param.setup_lines.iter().cloned())
            .collect();

        let cleanup_lines = lowered_params
            .iter()
            .rev()
            .flat_map(|param| param.cleanup_lines.iter().cloned())
            .collect();

        let jni_args = lowered_params
            .iter()
            .map(|param| param.jni_arg.clone())
            .collect();

        let ffi_name = naming::vtable_field_name(method.id.as_str()).into_string();

        JniCallbackMethod {
            ffi_name: ffi_name.clone(),
            jni_method_name: ffi_name,
            jni_signature: self.build_callback_jni_signature(&method.params, &abi_method.returns),
            c_params,
            setup_lines,
            cleanup_lines,
            jni_args,
            return_info,
        }
    }

    fn lower_async_callback_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
        jni_prefix: &str,
    ) -> JniAsyncCallbackMethod {
        let invoker_suffix = self.async_invoker_suffix(&abi_method.returns);
        let return_c_type = self.async_callback_return_c_type(&abi_method.returns);

        let lowered_params: Vec<LoweredCallbackParam> = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(&param.name, &param.type_expr, true))
            .collect();

        let c_params = lowered_params
            .iter()
            .flat_map(|param| param.c_params.iter().cloned())
            .collect();

        let setup_lines = lowered_params
            .iter()
            .flat_map(|param| param.setup_lines.iter().cloned())
            .collect();

        let cleanup_lines = lowered_params
            .iter()
            .rev()
            .flat_map(|param| param.cleanup_lines.iter().cloned())
            .collect();

        let jni_args = lowered_params
            .iter()
            .map(|param| param.jni_arg.clone())
            .collect();
        let ffi_name = naming::vtable_field_name(method.id.as_str()).into_string();

        JniAsyncCallbackMethod {
            ffi_name: ffi_name.clone(),
            jni_method_name: ffi_name,
            jni_signature: self.build_async_callback_jni_signature(&method.params),
            c_params,
            setup_lines,
            cleanup_lines,
            jni_args,
            return_c_type,
            invoker_jni_name: format!(
                "Java_{}_Native_invokeAsyncCallback{}",
                jni_prefix, invoker_suffix
            ),
            invoker_native_name: format!("invokeAsyncCallback{}", invoker_suffix),
        }
    }

    fn build_callback_jni_signature(&self, params: &[ParamDef], ret_shape: &ReturnShape) -> String {
        let params_sig = std::iter::once("J".to_string())
            .chain(
                params
                    .iter()
                    .map(|param| self.type_expr_jni_signature(&param.type_expr)),
            )
            .collect::<Vec<_>>()
            .join("");

        let return_sig = match &ret_shape.transport {
            None => "V".to_string(),
            Some(Transport::Scalar(origin)) => self.primitive_signature(origin.primitive()),
            Some(Transport::Handle { .. } | Transport::Callback { .. }) => "J".to_string(),
            Some(Transport::Span(_) | Transport::Composite(_)) => "[B".to_string(),
        };

        format!("({}){}", params_sig, return_sig)
    }

    fn build_async_callback_jni_signature(&self, params: &[ParamDef]) -> String {
        let params_sig = std::iter::once("J".to_string())
            .chain(
                params
                    .iter()
                    .map(|param| self.type_expr_jni_signature(&param.type_expr)),
            )
            .chain(["J".to_string(), "J".to_string()])
            .collect::<Vec<_>>()
            .join("");

        format!("({})V", params_sig)
    }

    fn sync_callback_return_info(
        &self,
        ret_shape: &ReturnShape,
        out_direct_param: Option<&AbiParam>,
        out_len_param: Option<&AbiParam>,
    ) -> Option<JniCallbackReturn> {
        let out_ptr_name = out_direct_param
            .map(|param| param.name.as_str().to_string())
            .unwrap_or_else(|| "out_ptr".to_string());

        let out_len_name = out_len_param
            .map(|param| param.name.as_str().to_string())
            .unwrap_or_else(|| "out_len".to_string());

        match &ret_shape.transport {
            None => None,
            Some(Transport::Scalar(origin)) => {
                let primitive = origin.primitive();
                Some(JniCallbackReturn {
                    jni_type: primitives::info(primitive).jni_type.to_string(),
                    jni_call_type: primitives::info(primitive).call_suffix.to_string(),
                    c_type: self.primitive_c_type(primitive),
                    is_wire_encoded: false,
                    out_ptr_name: Some(out_ptr_name),
                    out_len_name: None,
                })
            }
            Some(Transport::Handle { .. }) => Some(JniCallbackReturn {
                jni_type: "jlong".to_string(),
                jni_call_type: "Long".to_string(),
                c_type: "uint8_t*".to_string(),
                is_wire_encoded: false,
                out_ptr_name: Some(out_ptr_name),
                out_len_name: None,
            }),
            Some(Transport::Callback { .. }) => Some(JniCallbackReturn {
                jni_type: "jlong".to_string(),
                jni_call_type: "Long".to_string(),
                c_type: "uint8_t*".to_string(),
                is_wire_encoded: false,
                out_ptr_name: Some(out_ptr_name),
                out_len_name: None,
            }),
            Some(Transport::Span(_) | Transport::Composite(_)) => Some(JniCallbackReturn {
                jni_type: "jbyteArray".to_string(),
                jni_call_type: "Object".to_string(),
                c_type: "uint8_t*".to_string(),
                is_wire_encoded: true,
                out_ptr_name: Some(out_ptr_name),
                out_len_name: Some(out_len_name),
            }),
        }
    }

    fn async_callback_return_c_type(&self, ret_shape: &ReturnShape) -> Option<String> {
        match &ret_shape.transport {
            None => None,
            Some(Transport::Scalar(origin)) => {
                Some(self.c_return_type_for_abi(&AbiType::from(origin.primitive())))
            }
            Some(Transport::Handle { .. }) | Some(Transport::Callback { .. }) => {
                Some("void*".to_string())
            }
            Some(_) => Some("wire".to_string()),
        }
    }

    fn async_invoker_suffix(&self, ret_shape: &ReturnShape) -> String {
        match &ret_shape.transport {
            None => "Void".to_string(),
            Some(Transport::Scalar(origin)) => primitives::info(origin.primitive())
                .invoker_suffix
                .to_string(),
            Some(Transport::Handle { .. }) | Some(Transport::Callback { .. }) => {
                "Handle".to_string()
            }
            Some(_) => "Wire".to_string(),
        }
    }

    fn type_expr_jni_signature(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => self.primitive_signature(*p),
            TypeExpr::Void => "V".to_string(),
            _ => "Ljava/nio/ByteBuffer;".to_string(),
        }
    }

    fn lower_callback_param(
        &self,
        param_name: &ParamName,
        ty: &TypeExpr,
        is_async: bool,
    ) -> LoweredCallbackParam {
        match ty {
            TypeExpr::Primitive(p) => self.lower_callback_primitive(param_name, *p),
            TypeExpr::Option(_) => self.lower_callback_optional(param_name),
            TypeExpr::String | TypeExpr::Record(_) => {
                self.lower_callback_encoded(param_name, is_async)
            }
            _ => self.lower_callback_encoded(param_name, is_async),
        }
    }

    fn lower_callback_primitive(
        &self,
        param_name: &ParamName,
        primitive: PrimitiveType,
    ) -> LoweredCallbackParam {
        let model_primitive = primitive;
        let info = primitives::info(model_primitive);
        let c_type = info.c_type.to_string();
        let jni_arg = info
            .jni_cast
            .map(|cast| format!("{}{}", cast, param_name.as_str()))
            .unwrap_or_else(|| param_name.as_str().to_string());

        LoweredCallbackParam {
            c_params: vec![JniCallbackCParam {
                name: param_name.as_str().to_string(),
                c_type,
            }],
            setup_lines: Vec::new(),
            cleanup_lines: Vec::new(),
            jni_arg,
        }
    }

    fn lower_callback_optional(&self, param_name: &ParamName) -> LoweredCallbackParam {
        let ptr_name = format!("{}_ptr", param_name.as_str());
        let len_name = format!("{}_len", param_name.as_str());
        let buf_name = format!("buf_{}", param_name.as_str());

        let setup_lines = vec![
            format!("jobject {buf_name} = NULL;"),
            format!("if ({ptr_name} != NULL) {{"),
            format!(
                "    {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
            ),
            "}".to_string(),
        ];

        LoweredCallbackParam {
            c_params: vec![
                JniCallbackCParam {
                    name: ptr_name,
                    c_type: "const uint8_t*".to_string(),
                },
                JniCallbackCParam {
                    name: len_name,
                    c_type: "uintptr_t".to_string(),
                },
            ],
            setup_lines,
            cleanup_lines: vec![format!(
                "if ({buf_name} != NULL) (*env)->DeleteLocalRef(env, {buf_name});"
            )],
            jni_arg: buf_name,
        }
    }

    fn lower_callback_encoded(
        &self,
        param_name: &ParamName,
        is_async: bool,
    ) -> LoweredCallbackParam {
        let ptr_name = format!("{}_ptr", param_name.as_str());
        let len_name = format!("{}_len", param_name.as_str());
        let buf_name = format!("buf_{}", param_name.as_str());

        let setup_lines = if is_async {
            vec![
                format!("jobject {buf_name} = NULL;"),
                format!("if ({ptr_name} == NULL) {{ goto cleanup; }}"),
                format!(
                    "{buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!("if ({buf_name} == NULL) {{ goto cleanup; }}"),
            ]
        } else {
            vec![
                format!("jobject {buf_name} = NULL;"),
                format!("if ({ptr_name} == NULL) {{ status->code = 1; goto cleanup; }}"),
                format!(
                    "{buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!("if ({buf_name} == NULL) {{ status->code = 1; goto cleanup; }}"),
            ]
        };

        LoweredCallbackParam {
            c_params: vec![
                JniCallbackCParam {
                    name: ptr_name,
                    c_type: "const uint8_t*".to_string(),
                },
                JniCallbackCParam {
                    name: len_name,
                    c_type: "uintptr_t".to_string(),
                },
            ],
            setup_lines,
            cleanup_lines: vec![format!(
                "if ({buf_name} != NULL) (*env)->DeleteLocalRef(env, {buf_name});"
            )],
            jni_arg: buf_name,
        }
    }

    fn jni_type_signature(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "Z".to_string(),
            AbiType::I8 | AbiType::U8 => "B".to_string(),
            AbiType::I16 | AbiType::U16 => "S".to_string(),
            AbiType::I32 | AbiType::U32 => "I".to_string(),
            AbiType::I64 | AbiType::U64 => "J".to_string(),
            AbiType::F32 => "F".to_string(),
            AbiType::F64 => "D".to_string(),
            _ => "Ljava/nio/ByteBuffer;".to_string(),
        }
    }

    fn callback_param(&self, param: &AbiParam, is_async: bool) -> LoweredCallbackParam {
        let param_name = param.name.as_str();
        match &param.role {
            ParamRole::Input {
                transport: Transport::Scalar(_),
                ..
            } => self.lower_callback_direct_param(param_name, &param.abi_type),
            ParamRole::Input {
                transport: Transport::Span(_),
                ..
            }
            | ParamRole::Input {
                transport: Transport::Composite(_),
                ..
            } => self.lower_callback_encoded_param(param_name, is_async),
            _ => unreachable!("unsupported JNI callback param role: {:?}", param.role),
        }
    }

    fn lower_callback_direct_param(
        &self,
        param_name: &str,
        abi_type: &AbiType,
    ) -> LoweredCallbackParam {
        let c_type = self.c_return_type_for_abi(abi_type);
        let jni_arg = match abi_type {
            AbiType::Bool => format!("{} != 0", param_name),
            _ => param_name.to_string(),
        };

        LoweredCallbackParam {
            c_params: vec![JniCallbackCParam {
                name: param_name.to_string(),
                c_type,
            }],
            setup_lines: Vec::new(),
            cleanup_lines: Vec::new(),
            jni_arg,
        }
    }

    fn lower_callback_encoded_param(
        &self,
        param_name: &str,
        is_async: bool,
    ) -> LoweredCallbackParam {
        let ptr_name = format!("{}_ptr", param_name);
        let len_name = format!("{}_len", param_name);
        let buf_name = format!("buf_{}", param_name);

        let setup_lines = if is_async {
            vec![
                format!("jobject {buf_name} = NULL;"),
                format!("if ({ptr_name} == NULL) {{ goto cleanup; }}"),
                format!(
                    "{buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!("if ({buf_name} == NULL) {{ goto cleanup; }}"),
            ]
        } else {
            vec![
                format!("jobject {buf_name} = NULL;"),
                format!("if ({ptr_name} == NULL) {{ status->code = 1; goto cleanup; }}"),
                format!(
                    "{buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                ),
                format!("if ({buf_name} == NULL) {{ status->code = 1; goto cleanup; }}"),
            ]
        };

        LoweredCallbackParam {
            c_params: vec![
                JniCallbackCParam {
                    name: ptr_name,
                    c_type: "const uint8_t*".to_string(),
                },
                JniCallbackCParam {
                    name: len_name,
                    c_type: "uintptr_t".to_string(),
                },
            ],
            setup_lines,
            cleanup_lines: vec![format!(
                "if ({buf_name} != NULL) (*env)->DeleteLocalRef(env, {buf_name});"
            )],
            jni_arg: buf_name,
        }
    }

    fn callback_out_param(&self, param: &AbiParam) -> Option<JniCallbackCParam> {
        match &param.role {
            ParamRole::OutDirect | ParamRole::OutLen { .. } => Some(JniCallbackCParam {
                name: param.name.as_str().to_string(),
                c_type: format!("{} *", self.callback_abi_type_c(&param.abi_type)),
            }),
            _ => None,
        }
    }

    fn callback_primitive_c_type(&self, primitive: PrimitiveType) -> &'static str {
        match primitive {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "int8_t",
            PrimitiveType::U8 => "uint8_t",
            PrimitiveType::I16 => "int16_t",
            PrimitiveType::U16 => "uint16_t",
            PrimitiveType::I32 => "int32_t",
            PrimitiveType::U32 => "uint32_t",
            PrimitiveType::I64 => "int64_t",
            PrimitiveType::U64 => "uint64_t",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
            PrimitiveType::ISize => "intptr_t",
            PrimitiveType::USize => "uintptr_t",
        }
    }

    fn callback_abi_type_c(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Void => "void".to_string(),
            AbiType::Bool => "bool".to_string(),
            AbiType::I8 => "int8_t".to_string(),
            AbiType::U8 => "uint8_t".to_string(),
            AbiType::I16 => "int16_t".to_string(),
            AbiType::U16 => "uint16_t".to_string(),
            AbiType::I32 => "int32_t".to_string(),
            AbiType::U32 => "uint32_t".to_string(),
            AbiType::I64 => "int64_t".to_string(),
            AbiType::U64 => "uint64_t".to_string(),
            AbiType::F32 => "float".to_string(),
            AbiType::F64 => "double".to_string(),
            AbiType::ISize => "intptr_t".to_string(),
            AbiType::USize => "uintptr_t".to_string(),
            AbiType::Pointer(element) => {
                format!("{}*", self.callback_primitive_c_type(*element))
            }
            AbiType::InlineCallbackFn {
                params,
                return_type,
            } => {
                let param_types = std::iter::once("void*".to_string())
                    .chain(params.iter().map(|param| match param {
                        AbiType::Pointer(element) => {
                            format!("const {}*", self.callback_primitive_c_type(*element))
                        }
                        other => self.callback_abi_type_c(other),
                    }))
                    .collect::<Vec<_>>();
                let c_return = self.callback_abi_type_c(return_type);
                format!("{} (*)({})", c_return, param_types.join(", "))
            }
            AbiType::Handle(class_id) => format!("const struct {} *", class_id.as_str()),
            AbiType::CallbackHandle => "BoltFFICallbackHandle".to_string(),
            AbiType::Struct(record_id) => format!("___{}", record_id.as_str()),
        }
    }

    fn c_return_type_for_abi(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "bool".to_string(),
            AbiType::I8 => "int8_t".to_string(),
            AbiType::U8 => "uint8_t".to_string(),
            AbiType::I16 => "int16_t".to_string(),
            AbiType::U16 => "uint16_t".to_string(),
            AbiType::I32 => "int32_t".to_string(),
            AbiType::U32 => "uint32_t".to_string(),
            AbiType::I64 | AbiType::ISize => "int64_t".to_string(),
            AbiType::U64 | AbiType::USize => "uint64_t".to_string(),
            AbiType::F32 => "float".to_string(),
            AbiType::F64 => "double".to_string(),
            AbiType::Void
            | AbiType::Pointer(_)
            | AbiType::InlineCallbackFn { .. }
            | AbiType::Handle(_)
            | AbiType::CallbackHandle => "void".to_string(),
            AbiType::Struct(_) => "jlong".to_string(),
        }
    }

    fn jni_return_type_for_abi(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "jboolean".to_string(),
            AbiType::I8 | AbiType::U8 => "jbyte".to_string(),
            AbiType::I16 | AbiType::U16 => "jshort".to_string(),
            AbiType::I32 | AbiType::U32 => "jint".to_string(),
            AbiType::I64 | AbiType::U64 => "jlong".to_string(),
            AbiType::F32 => "jfloat".to_string(),
            AbiType::F64 => "jdouble".to_string(),
            _ => "jobject".to_string(),
        }
    }

    fn jni_call_type_for_abi(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 | AbiType::U8 => "Byte".to_string(),
            AbiType::I16 | AbiType::U16 => "Short".to_string(),
            AbiType::I32 | AbiType::U32 => "Int".to_string(),
            AbiType::I64 | AbiType::U64 => "Long".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn invoker_result_type(suffix: &str) -> Option<JniInvokerResult> {
        match suffix {
            "Void" => None,
            "Wire" => Some(JniInvokerResult {
                c_type: "wire".to_string(),
                jni_type: "jbyteArray".to_string(),
            }),
            "Bool" => Some(JniInvokerResult {
                c_type: "bool".to_string(),
                jni_type: "jboolean".to_string(),
            }),
            "I8" => Some(JniInvokerResult {
                c_type: "int8_t".to_string(),
                jni_type: "jbyte".to_string(),
            }),
            "I16" => Some(JniInvokerResult {
                c_type: "int16_t".to_string(),
                jni_type: "jshort".to_string(),
            }),
            "I32" => Some(JniInvokerResult {
                c_type: "int32_t".to_string(),
                jni_type: "jint".to_string(),
            }),
            "I64" => Some(JniInvokerResult {
                c_type: "int64_t".to_string(),
                jni_type: "jlong".to_string(),
            }),
            "Handle" => Some(JniInvokerResult {
                c_type: "void*".to_string(),
                jni_type: "jlong".to_string(),
            }),
            "F32" => Some(JniInvokerResult {
                c_type: "float".to_string(),
                jni_type: "jfloat".to_string(),
            }),
            "F64" => Some(JniInvokerResult {
                c_type: "double".to_string(),
                jni_type: "jdouble".to_string(),
            }),
            _ => Some(JniInvokerResult {
                c_type: "void*".to_string(),
                jni_type: "jobject".to_string(),
            }),
        }
    }

    fn invoker_suffix(&self, abi_type: &AbiType) -> String {
        match abi_type {
            AbiType::Bool => "Bool".to_string(),
            AbiType::I8 => "I8".to_string(),
            AbiType::I16 => "I16".to_string(),
            AbiType::I32 => "I32".to_string(),
            AbiType::I64 => "I64".to_string(),
            AbiType::F32 => "F32".to_string(),
            AbiType::F64 => "F64".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn collect_async_invokers(
        &self,
        callback_traits: &[JniCallbackTrait],
        jni_prefix: &str,
    ) -> Vec<JniAsyncCallbackInvoker> {
        let mut seen = HashSet::new();
        callback_traits
            .iter()
            .flat_map(|trait_view| &trait_view.async_methods)
            .filter_map(|method| {
                let suffix = method
                    .invoker_native_name
                    .strip_prefix("invokeAsyncCallback")
                    .map(|value| value.to_string())?;
                if seen.insert(suffix.clone()) {
                    Some(self.build_async_invoker(&suffix, jni_prefix))
                } else {
                    None
                }
            })
            .collect()
    }

    fn build_async_invoker(&self, suffix: &str, jni_prefix: &str) -> JniAsyncCallbackInvoker {
        JniAsyncCallbackInvoker {
            suffix: suffix.to_string(),
            jni_fn_name: format!("Java_{}_Native_invokeAsyncCallback{}", jni_prefix, suffix),
            result_type: Self::invoker_result_type(suffix),
        }
    }

    fn collect_closure_trampolines(
        &self,
        package_path: &str,
        used_callbacks: &HashSet<CallbackId>,
    ) -> Vec<JniClosureTrampoline> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|callback| matches!(callback.kind, CallbackKind::Closure))
            .filter(|callback| used_callbacks.contains(&callback.id))
            .map(|callback| self.lower_closure_trampoline(callback, package_path))
            .collect()
    }

    fn lower_closure_trampoline(
        &self,
        callback: &CallbackTraitDef,
        package_path: &str,
    ) -> JniClosureTrampoline {
        let signature_id = callback
            .id
            .as_str()
            .strip_prefix("__Closure_")
            .unwrap_or(callback.id.as_str())
            .to_string();

        let callbacks_class = format!("Closure{}Callbacks", signature_id);
        let callbacks_class_jni_path =
            format!("{}/{}", package_path.replace('.', "/"), callbacks_class);

        let method = callback
            .methods
            .iter()
            .find(|method| method.id.as_str() == "call");
        let params = method
            .map(|method| method.params.as_slice())
            .unwrap_or_default();

        let record_params = params
            .iter()
            .enumerate()
            .filter_map(|(index, param)| self.closure_record_param(index, &param.type_expr))
            .collect::<Vec<_>>();

        let c_params = self.closure_c_params(params);
        let jni_params_signature = self.closure_jni_params_signature(params);
        let jni_call_args = self.closure_jni_call_args(params);

        let return_info = method.and_then(|m| match &m.returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(self.closure_return_info(ty)),
            ReturnDef::Result { .. } => Some(JniClosureTrampolineReturn::wire_encoded()),
        });

        JniClosureTrampoline {
            trampoline_name: format!("trampoline_{}", signature_id),
            signature_id,
            callbacks_class_jni_path,
            c_params,
            jni_params_signature,
            jni_call_args,
            record_params,
            return_info,
        }
    }

    fn closure_return_info(&self, ty: &TypeExpr) -> JniClosureTrampolineReturn {
        match ty {
            TypeExpr::Primitive(p) => {
                let model_primitive = *p;
                let info = primitives::info(model_primitive);
                let c_type = info.c_type.to_string();
                let jni_call_method = format!("CallStatic{}Method", info.call_suffix);
                let jni_return_cast = format!("({})", c_type);
                let jni_signature = info.signature.to_string();
                JniClosureTrampolineReturn {
                    c_type,
                    jni_call_method,
                    jni_return_cast,
                    jni_signature,
                    strategy: TrampolineReturnStrategy::Direct,
                }
            }
            TypeExpr::Record(id) => {
                let abi_record = self.abi.records.iter().find(|r| r.id == *id);
                match abi_record {
                    Some(rec) if rec.is_blittable => JniClosureTrampolineReturn {
                        c_type: format!("___{}", id.as_str()),
                        jni_call_method: "CallStaticObjectMethod".to_string(),
                        jni_return_cast: String::new(),
                        jni_signature: "[B".to_string(),
                        strategy: TrampolineReturnStrategy::BlittableStruct {
                            struct_size: rec.size.unwrap_or(0),
                        },
                    },
                    _ => JniClosureTrampolineReturn::wire_encoded(),
                }
            }
            TypeExpr::String => JniClosureTrampolineReturn {
                c_type: "uint8_t*".to_string(),
                jni_call_method: "CallStaticObjectMethod".to_string(),
                jni_return_cast: String::new(),
                jni_signature: "[B".to_string(),
                strategy: TrampolineReturnStrategy::RawPointer,
            },
            TypeExpr::Enum(_)
            | TypeExpr::Bytes
            | TypeExpr::Vec(_)
            | TypeExpr::Option(_)
            | TypeExpr::Builtin(_) => JniClosureTrampolineReturn::wire_encoded(),
            TypeExpr::Handle(class_id) => JniClosureTrampolineReturn {
                c_type: format!("struct {}*", class_id.as_str()),
                jni_call_method: "CallStaticLongMethod".to_string(),
                jni_return_cast: format!("(struct {}*)(intptr_t)", class_id.as_str()),
                jni_signature: "J".to_string(),
                strategy: TrampolineReturnStrategy::Direct,
            },
            TypeExpr::Callback(id) => {
                let snake = naming::to_snake_case(id.as_str());
                let prefix = naming::ffi_prefix();
                JniClosureTrampolineReturn {
                    c_type: "BoltFFICallbackHandle".to_string(),
                    jni_call_method: "CallStaticLongMethod".to_string(),
                    jni_return_cast: String::new(),
                    jni_signature: "J".to_string(),
                    strategy: TrampolineReturnStrategy::CallbackHandle {
                        create_fn: format!("{}_create_{}_handle", prefix, snake),
                    },
                }
            }
            _ => JniClosureTrampolineReturn::wire_encoded(),
        }
    }

    fn closure_record_param(&self, index: usize, ty: &TypeExpr) -> Option<JniClosureRecordParam> {
        match ty {
            TypeExpr::Record(id) => {
                let c_type = NamingConvention::class_name(id.as_str());
                let size = self.record_struct_size(id).to_string();
                Some(JniClosureRecordParam {
                    index,
                    c_type,
                    size,
                })
            }
            TypeExpr::String => Some(JniClosureRecordParam {
                index,
                c_type: "String".to_string(),
                size: "0".to_string(),
            }),
            _ => None,
        }
    }

    fn closure_c_params(&self, params: &[ParamDef]) -> String {
        let items = params
            .iter()
            .enumerate()
            .map(|(index, param)| match param.type_expr {
                TypeExpr::Primitive(p) => format!("{} p{}", self.primitive_c_type(p), index),
                TypeExpr::Record(_) | TypeExpr::String => {
                    format!("const uint8_t* p{}_ptr, uintptr_t p{}_len", index, index)
                }
                _ => format!("void* p{}", index),
            })
            .collect::<Vec<_>>();

        if items.is_empty() {
            String::new()
        } else {
            format!(", {}", items.join(", "))
        }
    }

    fn closure_jni_params_signature(&self, params: &[ParamDef]) -> String {
        params
            .iter()
            .map(|param| match &param.type_expr {
                TypeExpr::Primitive(p) => self.primitive_signature(*p),
                TypeExpr::String | TypeExpr::Record(_) => "Ljava/nio/ByteBuffer;".to_string(),
                _ => "Ljava/lang/Object;".to_string(),
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn closure_jni_call_args(&self, params: &[ParamDef]) -> String {
        params
            .iter()
            .enumerate()
            .map(|(index, param)| match &param.type_expr {
                TypeExpr::Primitive(p) => format!("({})p{}", self.primitive_jni_cast(*p), index),
                TypeExpr::String | TypeExpr::Record(_) => format!("buf_p{}", index),
                _ => format!("(jlong)p{}", index),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn primitive_jni_cast(&self, primitive: PrimitiveType) -> &'static str {
        match primitive {
            PrimitiveType::Bool => "jboolean",
            PrimitiveType::I8 | PrimitiveType::U8 => "jbyte",
            PrimitiveType::I16 | PrimitiveType::U16 => "jshort",
            PrimitiveType::I32 | PrimitiveType::U32 => "jint",
            PrimitiveType::I64 | PrimitiveType::U64 => "jlong",
            PrimitiveType::F32 => "jfloat",
            PrimitiveType::F64 => "jdouble",
            PrimitiveType::ISize | PrimitiveType::USize => "jlong",
        }
    }
}

#[derive(Clone)]
struct RecordParamInfo {
    id: RecordId,
    struct_size: usize,
}

#[derive(Clone)]
struct DataEnumParamInfo {
    id: EnumId,
}

#[derive(Clone)]
struct LoweredCallbackParam {
    c_params: Vec<JniCallbackCParam>,
    setup_lines: Vec<String>,
    cleanup_lines: Vec<String>,
    jni_arg: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::abi::AbiContract;
    use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
    use crate::ir::types::PrimitiveType;
    use crate::ir::{
        CStyleVariant, CallbackId, CallbackKind, CallbackMethodDef, CallbackTraitDef, EnumDef,
        FieldDef, FieldName, MethodId, ParamDef, ParamPassing, RecordDef, RecordId, ReturnDef,
        VariantName,
    };

    fn test_lowerer() -> JniLowerer<'static> {
        static CONTRACT: std::sync::LazyLock<FfiContract> =
            std::sync::LazyLock::new(|| FfiContract {
                package: PackageInfo {
                    name: "test".to_string(),
                    version: None,
                },
                catalog: TypeCatalog::default(),
                functions: vec![],
            });
        static ABI: std::sync::LazyLock<AbiContract> =
            std::sync::LazyLock::new(|| IrLowerer::new(&CONTRACT).to_abi_contract());

        JniLowerer::new(
            &CONTRACT,
            &ABI,
            "com.test".to_string(),
            "Native".to_string(),
        )
    }

    #[test]
    fn primitive_c_type_bool_is_bool_not_uint8() {
        let lowerer = test_lowerer();
        assert_eq!(lowerer.primitive_c_type(PrimitiveType::Bool), "bool");
    }

    #[test]
    fn primitive_c_type_matches_cbindgen_for_all_types() {
        let lowerer = test_lowerer();
        let cases = [
            (PrimitiveType::Bool, "bool"),
            (PrimitiveType::I8, "int8_t"),
            (PrimitiveType::U8, "uint8_t"),
            (PrimitiveType::I16, "int16_t"),
            (PrimitiveType::U16, "uint16_t"),
            (PrimitiveType::I32, "int32_t"),
            (PrimitiveType::U32, "uint32_t"),
            (PrimitiveType::I64, "int64_t"),
            (PrimitiveType::U64, "uint64_t"),
            (PrimitiveType::ISize, "int64_t"),
            (PrimitiveType::USize, "uint64_t"),
            (PrimitiveType::F32, "float"),
            (PrimitiveType::F64, "double"),
        ];
        cases
            .iter()
            .for_each(|(prim, expected)| assert_eq!(lowerer.primitive_c_type(*prim), *expected));
    }

    #[test]
    fn c_return_type_for_abi_bool_is_bool() {
        let lowerer = test_lowerer();
        assert_eq!(lowerer.c_return_type_for_abi(&AbiType::Bool), "bool");
    }

    #[test]
    fn c_return_type_for_abi_matches_primitive_c_type() {
        let lowerer = test_lowerer();
        let abi_types = [
            (AbiType::Bool, PrimitiveType::Bool),
            (AbiType::I8, PrimitiveType::I8),
            (AbiType::U8, PrimitiveType::U8),
            (AbiType::I16, PrimitiveType::I16),
            (AbiType::U16, PrimitiveType::U16),
            (AbiType::I32, PrimitiveType::I32),
            (AbiType::U32, PrimitiveType::U32),
            (AbiType::I64, PrimitiveType::I64),
            (AbiType::U64, PrimitiveType::U64),
            (AbiType::F32, PrimitiveType::F32),
            (AbiType::F64, PrimitiveType::F64),
        ];
        abi_types.iter().for_each(|(abi, prim)| {
            assert_eq!(
                lowerer.c_return_type_for_abi(abi),
                lowerer.primitive_c_type(*prim),
                "mismatch for {:?}",
                abi
            );
        });
    }

    #[test]
    fn closure_return_primitive_is_direct() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Primitive(PrimitiveType::I32));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::Direct));
        assert_eq!(ret.c_type, "int32_t");
        assert_eq!(ret.jni_signature, "I");
    }

    #[test]
    fn closure_return_string_is_raw_pointer() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::String);
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::RawPointer));
        assert_eq!(ret.c_type, "uint8_t*");
        assert_eq!(ret.jni_signature, "[B");
    }

    #[test]
    fn closure_return_record_is_wire_encoded() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Record("Point".into()));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::WireBuffer));
        assert_eq!(ret.c_type, "FfiBuf_u8");
    }

    #[test]
    fn closure_return_enum_is_wire_encoded() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Enum("Color".into()));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::WireBuffer));
        assert_eq!(ret.c_type, "FfiBuf_u8");
    }

    #[test]
    fn closure_return_bytes_is_wire_encoded() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Bytes);
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::WireBuffer));
        assert_eq!(ret.c_type, "FfiBuf_u8");
    }

    #[test]
    fn closure_return_vec_is_wire_encoded() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(
            PrimitiveType::I32,
        ))));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::WireBuffer));
        assert_eq!(ret.c_type, "FfiBuf_u8");
    }

    #[test]
    fn closure_return_option_is_wire_encoded() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Option(Box::new(TypeExpr::Primitive(
            PrimitiveType::I32,
        ))));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::WireBuffer));
        assert_eq!(ret.c_type, "FfiBuf_u8");
    }

    #[test]
    fn closure_return_builtin_is_wire_encoded() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Builtin("Duration".into()));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::WireBuffer));
        assert_eq!(ret.c_type, "FfiBuf_u8");
    }

    #[test]
    fn closure_return_handle_uses_long_with_pointer_cast() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Handle("Player".into()));
        assert!(matches!(ret.strategy, TrampolineReturnStrategy::Direct));
        assert_eq!(ret.c_type, "struct Player*");
        assert_eq!(ret.jni_call_method, "CallStaticLongMethod");
        assert_eq!(ret.jni_return_cast, "(struct Player*)(intptr_t)");
        assert_eq!(ret.jni_signature, "J");
    }

    #[test]
    fn closure_return_callback_uses_create_handle() {
        let lowerer = test_lowerer();
        let ret = lowerer.closure_return_info(&TypeExpr::Callback("Listener".into()));
        assert!(matches!(
            ret.strategy,
            TrampolineReturnStrategy::CallbackHandle { .. }
        ));
        assert_eq!(ret.c_type, "BoltFFICallbackHandle");
        assert_eq!(ret.jni_call_method, "CallStaticLongMethod");
        assert_eq!(ret.jni_signature, "J");
        match &ret.strategy {
            TrampolineReturnStrategy::CallbackHandle { create_fn } => {
                assert_eq!(create_fn, "boltffi_create_listener_handle");
            }
            _ => panic!("expected CallbackHandle strategy"),
        }
    }

    fn contract_with_blittable_point() -> FfiContract {
        let mut catalog = TypeCatalog::default();
        catalog.insert_record(RecordDef {
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
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog,
            functions: vec![],
        }
    }

    fn contract_with_c_style_enum(tag_type: PrimitiveType) -> FfiContract {
        let mut catalog = TypeCatalog::default();
        catalog.insert_enum(EnumDef {
            id: "Status".into(),
            repr: EnumRepr::CStyle {
                tag_type,
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
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog,
            functions: vec![],
        }
    }

    fn lowerer_from_contract(contract: &FfiContract) -> JniLowerer<'_> {
        let abi = IrLowerer::new(contract).to_abi_contract();
        let abi_leaked: &'static AbiContract = Box::leak(Box::new(abi));
        JniLowerer::new(
            contract,
            abi_leaked,
            "com.test".to_string(),
            "Native".to_string(),
        )
    }

    #[test]
    fn closure_return_blittable_record_uses_struct_strategy() {
        let mut contract = contract_with_blittable_point();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("__Closure_PointToPoint"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params: vec![ParamDef {
                    name: "p".into(),
                    type_expr: TypeExpr::Record(RecordId::new("Point")),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::Record(RecordId::new("Point"))),
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        });
        let lowerer = lowerer_from_contract(&contract);
        let ret = lowerer.closure_return_info(&TypeExpr::Record(RecordId::new("Point")));
        assert!(
            matches!(
                ret.strategy,
                TrampolineReturnStrategy::BlittableStruct { .. }
            ),
            "blittable record should use BlittableStruct, got {:?}",
            std::mem::discriminant(&ret.strategy)
        );
        assert_eq!(ret.c_type, "___Point");
    }

    #[test]
    fn inline_callback_fn_c_type_includes_struct_return() {
        let abi_type = AbiType::InlineCallbackFn {
            params: vec![AbiType::Pointer(PrimitiveType::U8), AbiType::USize],
            return_type: Box::new(AbiType::Struct(RecordId::new("Point"))),
        };
        let lowerer = test_lowerer();
        let c_type = lowerer.callback_abi_type_c(&abi_type);
        assert_eq!(c_type, "___Point (*)(void*, const uint8_t*, uintptr_t)");
    }

    #[test]
    fn inline_callback_fn_c_type_includes_pointer_return_for_string() {
        let abi_type = AbiType::InlineCallbackFn {
            params: vec![AbiType::Pointer(PrimitiveType::U8), AbiType::USize],
            return_type: Box::new(AbiType::Pointer(PrimitiveType::U8)),
        };
        let lowerer = test_lowerer();
        let c_type = lowerer.callback_abi_type_c(&abi_type);
        assert_eq!(c_type, "uint8_t* (*)(void*, const uint8_t*, uintptr_t)");
    }

    #[test]
    fn inline_callback_fn_c_type_void_return() {
        let abi_type = AbiType::InlineCallbackFn {
            params: vec![AbiType::I32],
            return_type: Box::new(AbiType::Void),
        };
        let lowerer = test_lowerer();
        let c_type = lowerer.callback_abi_type_c(&abi_type);
        assert_eq!(c_type, "void (*)(void*, int32_t)");
    }

    #[test]
    fn return_meta_c_style_enum_is_direct_not_wire_encoded() {
        let contract = contract_with_c_style_enum(PrimitiveType::I32);
        let lowerer = lowerer_from_contract(&contract);
        let meta = lowerer.return_meta(&ReturnDef::Value(TypeExpr::Enum("Status".into())));
        assert!(meta.is_direct);
        assert_eq!(meta.jni_return_type, "jint");
        assert_eq!(meta.jni_c_return_type, "int32_t");
    }

    #[test]
    fn return_meta_c_style_enum_u8_uses_jbyte() {
        let contract = contract_with_c_style_enum(PrimitiveType::U8);
        let lowerer = lowerer_from_contract(&contract);
        let meta = lowerer.return_meta(&ReturnDef::Value(TypeExpr::Enum("Status".into())));
        assert!(meta.is_direct);
        assert_eq!(meta.jni_return_type, "jbyte");
        assert_eq!(meta.jni_c_return_type, "uint8_t");
    }

    #[test]
    fn return_meta_custom_type_is_wire_encoded() {
        let lowerer = test_lowerer();
        let meta = lowerer.return_meta(&ReturnDef::Value(TypeExpr::Custom("Email".into())));
        assert!(!meta.is_direct);
        assert_eq!(meta.jni_return_type, "jbyteArray");
    }

    #[test]
    fn return_meta_data_enum_is_wire_encoded() {
        let lowerer = test_lowerer();
        let meta = lowerer.return_meta(&ReturnDef::Value(TypeExpr::Enum("Shape".into())));
        assert!(!meta.is_direct);
        assert_eq!(meta.jni_return_type, "jbyteArray");
    }
}
