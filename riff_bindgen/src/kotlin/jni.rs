use std::collections::HashSet;

use askama::Template;
use riff_ffi_rules::naming;

use super::NamingConvention;
use super::call_plan::{AsyncCallPlan, WireFunctionPlan};
use super::marshal::{JniParamInfo, JniReturnKind, OptionView, ResultView};
use super::primitives;
use super::return_abi::ReturnAbi;
use crate::model::{
    CallbackTrait, Class, ClosureSignature, Constructor, Function, Method, Module, Primitive,
    ReturnType, TraitMethod, TraitMethodParam, Type,
};

#[derive(Template)]
#[template(path = "kotlin/jni_wire_function.txt", escape = "none")]
pub struct JniWireFunctionTemplate {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParamInfo>,
    pub return_abi: ReturnAbi,
}

impl JniWireFunctionTemplate {
    pub fn from_function(func: &Function, jni_prefix: &str, module: &Module) -> Self {
        let prefix = naming::ffi_prefix();
        let ffi_name = format!("{}_{}", prefix, func.name);
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParamInfo> = func
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let jni_params = if params.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                params
                    .iter()
                    .map(|p| p.jni_param_decl())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let return_abi = ReturnAbi::from_return_type(&func.returns, module);

        Self {
            ffi_name,
            jni_name,
            jni_params,
            params,
            return_abi,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/jni_wire_method.txt", escape = "none")]
pub struct JniWireMethodTemplate {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParamInfo>,
    pub return_abi: ReturnAbi,
    pub include_handle: bool,
}

impl JniWireMethodTemplate {
    pub fn from_method(class: &Class, method: &Method, jni_prefix: &str, module: &Module) -> Self {
        let ffi_name = naming::method_ffi_name(&class.name, &method.name);
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParamInfo> = method
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let jni_params = if params.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                params
                    .iter()
                    .map(|param| param.jni_param_decl())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let return_abi = ReturnAbi::from_return_type(&method.returns, module);

        Self {
            ffi_name,
            jni_name,
            jni_params,
            params,
            return_abi,
            include_handle: !method.is_static(),
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/jni_wire_ctor.txt", escape = "none")]
pub struct JniWireCtorTemplate {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParamInfo>,
}

impl JniWireCtorTemplate {
    pub fn from_constructor(
        class: &Class,
        ctor: &Constructor,
        jni_prefix: &str,
        module: &Module,
    ) -> Self {
        let ffi_prefix = naming::class_ffi_prefix(&class.name);
        let ffi_name = if ctor.is_default() {
            format!("{}_new", ffi_prefix)
        } else {
            naming::method_ffi_name(&class.name, &ctor.name)
        };

        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let params: Vec<JniParamInfo> = ctor
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let jni_params = if params.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                params
                    .iter()
                    .map(|param| param.jni_param_decl())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        Self {
            ffi_name,
            jni_name,
            jni_params,
            params,
        }
    }
}

#[derive(Template)]
#[template(path = "kotlin/jni_glue.txt", escape = "none")]
pub struct JniGlueTemplate {
    pub prefix: String,
    pub jni_prefix: String,
    pub package_path: String,
    pub module_name: String,
    pub class_name: String,
    pub has_async: bool,
    pub has_async_callbacks: bool,
    pub functions: Vec<JniFunctionView>,
    pub wire_functions: Vec<String>,
    pub async_functions: Vec<JniAsyncFunctionView>,
    pub classes: Vec<JniClassView>,
    pub callback_traits: Vec<JniCallbackTraitView>,
    pub async_callback_invokers: Vec<AsyncCallbackInvoker>,
    pub closure_trampolines: Vec<ClosureTrampolineView>,
}

pub struct ClosureTrampolineView {
    pub trampoline_name: String,
    pub signature_id: String,
    pub c_params: String,
    pub jni_signature: String,
    pub jni_call_args: String,
    pub invoke_method_name: String,
    pub record_params: Vec<ClosureRecordParam>,
}

pub struct ClosureRecordParam {
    pub index: usize,
    pub c_type: String,
    pub size: String,
}

pub struct AsyncCallbackInvoker {
    pub suffix: String,
    pub jni_fn_name: String,
    pub c_result_type: String,
    pub jni_result_type: String,
    pub has_result: bool,
}

pub struct JniCallbackTraitView {
    pub trait_name: String,
    pub vtable_type: String,
    pub register_fn: String,
    pub callbacks_class: String,
    pub sync_methods: Vec<JniCallbackMethodView>,
    pub async_methods: Vec<JniAsyncCallbackMethodView>,
}

pub struct JniAsyncCallbackMethodView {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub c_params: Vec<JniCallbackCParamView>,
    pub setup_lines: Vec<String>,
    pub jni_args: Vec<String>,
    pub has_return: bool,
    pub return_c_type: String,
    pub invoker_jni_name: String,
    pub invoker_native_name: String,
}

pub struct JniCallbackMethodView {
    pub ffi_name: String,
    pub jni_method_name: String,
    pub jni_signature: String,
    pub jni_return_type: String,
    pub jni_call_type: String,
    pub c_return_type: String,
    pub has_return: bool,
    pub c_params: Vec<JniCallbackCParamView>,
    pub setup_lines: Vec<String>,
    pub jni_args: Vec<String>,
}

#[derive(Clone)]
pub struct JniCallbackCParamView {
    pub name: String,
    pub c_type: String,
}

struct LoweredCallbackParam {
    c_params: Vec<JniCallbackCParamView>,
    setup_lines: Vec<String>,
    jni_arg: String,
}

impl LoweredCallbackParam {
    fn lower_sync(param_name: &str, ty: &Type) -> Self {
        Self::lower(param_name, ty, CallbackFailureMode::Sync)
    }

    fn lower_async(param_name: &str, ty: &Type) -> Self {
        Self::lower(param_name, ty, CallbackFailureMode::Async)
    }

    fn lower(param_name: &str, ty: &Type, failure_mode: CallbackFailureMode) -> Self {
        match ty {
            Type::Primitive(primitive) => Self {
                c_params: vec![JniCallbackCParamView {
                    name: param_name.to_string(),
                    c_type: primitive.c_type_name().to_string(),
                }],
                setup_lines: Vec::new(),
                jni_arg: primitives::info(*primitive)
                    .jni_cast
                    .map(|cast| format!("{}{}", cast, param_name))
                    .unwrap_or_else(|| param_name.to_string()),
            },
            Type::Option(_) => {
                let ptr_name = format!("{param_name}_ptr");
                let len_name = format!("{param_name}_len");
                let buf_name = format!("buf_{param_name}");

                let setup_lines = vec![
                    format!("jobject {buf_name} = NULL;"),
                    format!("if ({ptr_name} != NULL) {{"),
                    format!(
                        "    {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                    ),
                    "}".to_string(),
                ];

                Self {
                    c_params: vec![
                        JniCallbackCParamView {
                            name: ptr_name,
                            c_type: "const uint8_t*".to_string(),
                        },
                        JniCallbackCParamView {
                            name: len_name,
                            c_type: "uintptr_t".to_string(),
                        },
                    ],
                    setup_lines,
                    jni_arg: buf_name,
                }
            }
            _ => {
                let ptr_name = format!("{param_name}_ptr");
                let len_name = format!("{param_name}_len");
                let buf_name = format!("buf_{param_name}");

                let setup_lines = match failure_mode {
                    CallbackFailureMode::Sync => vec![
                        format!(
                            "if ({ptr_name} == NULL) {{ status->code = 1; if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                        ),
                        format!(
                            "jobject {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                        ),
                        format!(
                            "if ({buf_name} == NULL) {{ status->code = 1; if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                        ),
                    ],
                    CallbackFailureMode::Async => vec![
                        format!(
                            "if ({ptr_name} == NULL) {{ if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                        ),
                        format!(
                            "jobject {buf_name} = (*env)->NewDirectByteBuffer(env, (void*){ptr_name}, (jlong){len_name});"
                        ),
                        format!(
                            "if ({buf_name} == NULL) {{ if (attached) (*g_jvm)->DetachCurrentThread(g_jvm); return; }}"
                        ),
                    ],
                };

                Self {
                    c_params: vec![
                        JniCallbackCParamView {
                            name: ptr_name,
                            c_type: "const uint8_t*".to_string(),
                        },
                        JniCallbackCParamView {
                            name: len_name,
                            c_type: "uintptr_t".to_string(),
                        },
                    ],
                    setup_lines,
                    jni_arg: buf_name,
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum CallbackFailureMode {
    Sync,
    Async,
}

pub struct JniAsyncFunctionView {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub jni_create_name: String,
    pub jni_poll_name: String,
    pub jni_complete_name: String,
    pub jni_cancel_name: String,
    pub jni_free_name: String,
    pub jni_params: String,
    pub jni_complete_return: String,
    pub jni_complete_c_type: String,
    pub complete_is_void: bool,
    pub complete_is_string: bool,
    pub complete_is_vec: bool,
    pub complete_is_record: bool,
    pub complete_is_result: bool,
    pub vec_buf_type: String,
    pub vec_free_fn: String,
    pub vec_jni_array_type: String,
    pub vec_new_array_fn: String,
    pub vec_set_array_fn: String,
    pub vec_jni_element_type: String,
    pub record_c_type: String,
    pub record_struct_size: usize,
    pub result_ok_is_void: bool,
    pub result_ok_is_string: bool,
    pub result_ok_c_type: String,
    pub result_ok_jni_type: String,
    pub result_err_is_string: bool,
    pub result_err_struct_size: usize,
    pub params: Vec<JniParamInfo>,
}

enum VecReturnKind {
    None,
    Primitive(PrimitiveVecInfo),
    Record(RecordVecInfo),
}

enum OptionVecReturnKind {
    None,
    Primitive(OptionPrimitiveVecInfo),
    Record(OptionRecordVecInfo),
    VecString(VecStringInfo),
    VecEnum(VecEnumInfo),
}

struct VecStringInfo {
    buf_type: String,
    free_fn: String,
}

struct VecEnumInfo {
    buf_type: String,
    free_fn: String,
}

struct PrimitiveVecInfo {
    c_type: String,
    buf_type: String,
    free_fn: String,
    jni_array_type: String,
    new_array_fn: String,
}

struct RecordVecInfo {
    buf_type: String,
    free_fn: String,
    struct_size: usize,
}

struct OptionPrimitiveVecInfo {
    c_type: String,
    buf_type: String,
    free_fn: String,
    jni_array_type: String,
    new_array_fn: String,
}

struct OptionRecordVecInfo {
    buf_type: String,
    free_fn: String,
    struct_size: usize,
}

impl VecReturnKind {
    fn from_returns(returns: &ReturnType, _func_name: &str, module: &Module) -> Self {
        let Some(Type::Vec(inner)) = returns.ok_type() else {
            return Self::None;
        };

        match inner.as_ref() {
            Type::Primitive(primitive) => {
                let cbindgen_name = primitive.cbindgen_name();
                let pinfo = primitives::info(*primitive);
                Self::Primitive(PrimitiveVecInfo {
                    c_type: primitive.c_type_name().to_string(),
                    buf_type: format!("FfiBuf_{}", cbindgen_name),
                    free_fn: format!("riff_free_buf_{}", cbindgen_name),
                    jni_array_type: pinfo.array_type.to_string(),
                    new_array_fn: pinfo.new_array_fn.to_string(),
                })
            }
            Type::Record(record_name) => {
                let struct_size = module
                    .records
                    .iter()
                    .find(|record| record.name == *record_name)
                    .map(|record| record.struct_size().as_usize())
                    .unwrap_or(0);

                Self::Record(RecordVecInfo {
                    buf_type: format!("FfiBuf_{}", record_name),
                    free_fn: format!("riff_free_buf_{}", record_name),
                    struct_size,
                })
            }
            _ => Self::None,
        }
    }

    fn is_primitive(&self) -> bool {
        matches!(self, Self::Primitive(_))
    }

    fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }
}

impl OptionVecReturnKind {
    fn from_returns(returns: &ReturnType, _func_name: &str, module: &Module) -> Self {
        let Some(Type::Option(inner)) = returns.ok_type() else {
            return Self::None;
        };
        let Type::Vec(inner) = inner.as_ref() else {
            return Self::None;
        };

        match inner.as_ref() {
            Type::Primitive(primitive) => {
                let cbindgen_name = primitive.cbindgen_name();
                let pinfo = primitives::info(*primitive);
                Self::Primitive(OptionPrimitiveVecInfo {
                    c_type: primitive.c_type_name().to_string(),
                    buf_type: format!("FfiBuf_{}", cbindgen_name),
                    free_fn: format!("riff_free_buf_{}", cbindgen_name),
                    jni_array_type: pinfo.array_type.to_string(),
                    new_array_fn: pinfo.new_array_fn.to_string(),
                })
            }
            Type::Record(record_name) => {
                let struct_size = module
                    .records
                    .iter()
                    .find(|record| record.name == *record_name)
                    .map(|record| record.struct_size().as_usize())
                    .unwrap_or(0);

                Self::Record(OptionRecordVecInfo {
                    buf_type: format!("FfiBuf_{}", record_name),
                    free_fn: format!("riff_free_buf_{}", record_name),
                    struct_size,
                })
            }
            Type::String => Self::VecString(VecStringInfo {
                buf_type: "FfiBuf_FfiString".to_string(),
                free_fn: "riff_free_buf_FfiString".to_string(),
            }),
            Type::Enum(enum_name) => {
                let is_data_enum = module
                    .enums
                    .iter()
                    .any(|e| e.name == *enum_name && e.is_data_enum());
                if is_data_enum {
                    Self::None
                } else {
                    Self::VecEnum(VecEnumInfo {
                        buf_type: format!("FfiBuf_{}", enum_name),
                        free_fn: format!("riff_free_buf_{}", enum_name),
                    })
                }
            }
            _ => Self::None,
        }
    }
}

pub struct JniFunctionView {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_return: String,
    pub jni_params: String,
    pub return_kind: JniReturnKind,
    pub params: Vec<JniParamInfo>,
    pub is_wire_encoded: bool,
    pub is_vec: bool,
    pub is_vec_record: bool,
    pub is_data_enum_return: bool,
    pub data_enum_return_name: String,
    pub data_enum_return_size: usize,
    pub vec_buf_type: String,
    pub vec_free_fn: String,
    pub vec_c_type: String,
    pub vec_jni_array_type: String,
    pub vec_new_array_fn: String,
    pub vec_struct_size: usize,
    pub option_vec_buf_type: String,
    pub option_vec_free_fn: String,
    pub option_vec_c_type: String,
    pub option_vec_jni_array_type: String,
    pub option_vec_new_array_fn: String,
    pub option_vec_struct_size: usize,
    pub option: Option<OptionView>,
    pub result: Option<ResultView>,
}

pub struct JniClassView {
    pub ffi_prefix: String,
    pub jni_ffi_prefix: String,
    pub jni_prefix: String,
    pub ctors: Vec<String>,
    pub wire_methods: Vec<String>,
    pub async_methods: Vec<JniAsyncFunctionView>,
}

pub struct JniGenerator;

impl JniGenerator {
    pub fn generate(module: &Module, package: &str) -> String {
        let class_name = NamingConvention::class_name(&module.name);
        let template = JniGlueTemplate::from_module_with_class_name(module, package, &class_name);
        template.render().expect("JNI template render failed")
    }

    pub fn generate_with_class_name(module: &Module, package: &str, class_name: &str) -> String {
        let template = JniGlueTemplate::from_module_with_class_name(module, package, class_name);
        template.render().expect("JNI template render failed")
    }
}

impl JniGlueTemplate {
    pub fn from_module_with_class_name(module: &Module, package: &str, class_name: &str) -> Self {
        let prefix = naming::ffi_prefix().to_string();
        let jni_prefix = package
            .replace('_', "_1")
            .replace('.', "_")
            .replace('-', "_1");
        let package_path = package.replace('.', "/");

        let functions: Vec<JniFunctionView> = module
            .functions
            .iter()
            .filter(|func| !func.is_async && Self::is_primitive_return(func))
            .map(|func| Self::map_function(func, &prefix, &jni_prefix, module))
            .collect();

        let wire_functions: Vec<String> = module
            .functions
            .iter()
            .filter(|func| !func.is_async && !Self::is_primitive_return(func))
            .map(|func| {
                JniWireFunctionTemplate::from_function(func, &jni_prefix, module)
                    .render()
                    .unwrap()
            })
            .collect();

        let async_functions: Vec<JniAsyncFunctionView> = module
            .functions
            .iter()
            .filter(|func| func.is_async && Self::is_supported_async_function(func, module))
            .map(|func| Self::map_async_function(func, &jni_prefix, module))
            .collect();

        let classes: Vec<JniClassView> = module
            .classes
            .iter()
            .map(|c| Self::map_class(c, &prefix, &jni_prefix, module))
            .collect();

        let callback_traits: Vec<JniCallbackTraitView> = module
            .callback_traits
            .iter()
            .filter(|t| t.sync_methods().count() > 0 || t.async_methods().count() > 0)
            .map(|t| Self::map_callback_trait(t, &package_path, &jni_prefix))
            .collect();

        let has_async_callbacks = callback_traits.iter().any(|t| !t.async_methods.is_empty());

        let async_callback_invokers = Self::collect_async_invokers(&callback_traits, &jni_prefix);

        let closure_trampolines = Self::collect_closure_trampolines(module, &package_path);

        let has_async = !async_functions.is_empty()
            || classes.iter().any(|c| !c.async_methods.is_empty())
            || !callback_traits.is_empty();

        Self {
            prefix,
            jni_prefix: jni_prefix.clone(),
            package_path,
            module_name: module.name.clone(),
            class_name: class_name.to_string(),
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

    fn collect_async_invokers(
        callback_traits: &[JniCallbackTraitView],
        jni_prefix: &str,
    ) -> Vec<AsyncCallbackInvoker> {
        let mut seen = HashSet::new();
        callback_traits
            .iter()
            .flat_map(|t| &t.async_methods)
            .filter_map(|m| {
                let suffix = m.invoker_native_name.strip_prefix("invokeAsyncCallback")?;
                if seen.insert(suffix.to_string()) {
                    Some(Self::build_async_invoker(suffix, jni_prefix))
                } else {
                    None
                }
            })
            .collect()
    }

    fn build_async_invoker(suffix: &str, jni_prefix: &str) -> AsyncCallbackInvoker {
        let (c_result_type, jni_result_type, has_result) = match suffix {
            "Void" => ("void".to_string(), "void".to_string(), false),
            "Bool" => ("uint8_t".to_string(), "jboolean".to_string(), true),
            "I8" => ("int8_t".to_string(), "jbyte".to_string(), true),
            "I16" => ("int16_t".to_string(), "jshort".to_string(), true),
            "I32" => ("int32_t".to_string(), "jint".to_string(), true),
            "I64" => ("int64_t".to_string(), "jlong".to_string(), true),
            "F32" => ("float".to_string(), "jfloat".to_string(), true),
            "F64" => ("double".to_string(), "jdouble".to_string(), true),
            _ => ("void*".to_string(), "jobject".to_string(), true),
        };

        AsyncCallbackInvoker {
            suffix: suffix.to_string(),
            jni_fn_name: format!("Java_{}_Native_invokeAsyncCallback{}", jni_prefix, suffix),
            c_result_type,
            jni_result_type,
            has_result,
        }
    }

    fn map_callback_trait(
        callback_trait: &CallbackTrait,
        package_path: &str,
        jni_prefix: &str,
    ) -> JniCallbackTraitView {
        let trait_name = NamingConvention::class_name(&callback_trait.name);
        let callbacks_class = format!("{}Callbacks", trait_name);

        let sync_methods: Vec<JniCallbackMethodView> = callback_trait
            .sync_methods()
            .filter(|method| Self::is_supported_callback_method(method))
            .map(Self::map_sync_callback_method)
            .collect();

        let async_methods: Vec<JniAsyncCallbackMethodView> = callback_trait
            .async_methods()
            .filter(|method| Self::is_supported_callback_method(method))
            .map(|method| Self::map_async_callback_method(method, &trait_name, jni_prefix))
            .collect();

        JniCallbackTraitView {
            trait_name: trait_name.clone(),
            vtable_type: naming::callback_vtable_name(&callback_trait.name),
            register_fn: naming::callback_register_fn(&callback_trait.name),
            callbacks_class: format!("{}/{}", package_path, callbacks_class),
            sync_methods,
            async_methods,
        }
    }

    fn map_sync_callback_method(method: &TraitMethod) -> JniCallbackMethodView {
        let ffi_name = naming::to_snake_case(&method.name);
        let has_return = method.has_return();

        let (jni_return_type, jni_call_type, c_return_type) = method
            .returns
            .ok_type()
            .map(|ty| {
                (
                    Self::jni_call_return_type(ty),
                    Self::jni_call_method_suffix(ty),
                    Self::c_type_for_callback(ty),
                )
            })
            .unwrap_or(("void".to_string(), "Void".to_string(), "void".to_string()));

        let lowered_params: Vec<LoweredCallbackParam> = method
            .inputs
            .iter()
            .map(|param| LoweredCallbackParam::lower_sync(&param.name, &param.param_type))
            .collect();

        let jni_signature = Self::build_jni_signature(&method.inputs, &method.returns);

        JniCallbackMethodView {
            jni_method_name: ffi_name.clone(),
            ffi_name,
            jni_signature,
            jni_return_type,
            jni_call_type,
            c_return_type,
            has_return,
            c_params: lowered_params
                .iter()
                .flat_map(|param| param.c_params.iter().cloned())
                .collect(),
            setup_lines: lowered_params
                .iter()
                .flat_map(|param| param.setup_lines.iter().cloned())
                .collect(),
            jni_args: lowered_params
                .iter()
                .map(|param| param.jni_arg.clone())
                .collect(),
        }
    }

    fn map_async_callback_method(
        method: &TraitMethod,
        _trait_name: &str,
        jni_prefix: &str,
    ) -> JniAsyncCallbackMethodView {
        let ffi_name = naming::to_snake_case(&method.name);
        let has_return = method.has_return();

        let return_c_type = method
            .returns
            .ok_type()
            .map(Self::c_type_for_callback)
            .unwrap_or_else(|| "void".to_string());

        let lowered_params: Vec<LoweredCallbackParam> = method
            .inputs
            .iter()
            .map(|param| LoweredCallbackParam::lower_async(&param.name, &param.param_type))
            .collect();

        let jni_signature = Self::build_async_callback_jni_signature(&method.inputs);
        let invoker_suffix = Self::async_invoker_suffix(&method.returns);

        JniAsyncCallbackMethodView {
            jni_method_name: ffi_name.clone(),
            ffi_name,
            jni_signature,
            c_params: lowered_params
                .iter()
                .flat_map(|param| param.c_params.iter().cloned())
                .collect(),
            setup_lines: lowered_params
                .iter()
                .flat_map(|param| param.setup_lines.iter().cloned())
                .collect(),
            jni_args: lowered_params
                .iter()
                .map(|param| param.jni_arg.clone())
                .collect(),
            has_return,
            return_c_type,
            invoker_jni_name: format!(
                "Java_{}_Native_invokeAsyncCallback{}",
                jni_prefix, invoker_suffix
            ),
            invoker_native_name: format!("invokeAsyncCallback{}", invoker_suffix),
        }
    }

    fn build_async_callback_jni_signature(inputs: &[TraitMethodParam]) -> String {
        let params_sig: String = std::iter::once("J".to_string())
            .chain(
                inputs
                    .iter()
                    .map(|p| Self::jni_type_signature(&p.param_type)),
            )
            .chain(["J".to_string(), "J".to_string()])
            .collect();
        format!("({})V", params_sig)
    }

    fn async_invoker_suffix(returns: &ReturnType) -> String {
        match returns.ok_type() {
            None => "Void".to_string(),
            Some(Type::Void) => "Void".to_string(),
            Some(Type::Primitive(p)) => primitives::info(*p).invoker_suffix.to_string(),
            Some(Type::String) => "String".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn is_supported_callback_method(method: &TraitMethod) -> bool {
        let supported_return = matches!(
            method.returns.ok_type(),
            None | Some(Type::Void) | Some(Type::Primitive(_))
        );

        let supported_params = method
            .inputs
            .iter()
            .all(|param| Self::is_supported_callback_param(&param.param_type));

        supported_return && supported_params
    }

    fn is_supported_callback_param(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Primitive(_)
                | Type::String
                | Type::Bytes
                | Type::Record(_)
                | Type::Enum(_)
                | Type::Vec(_)
                | Type::Option(_)
                | Type::Result { .. }
        )
    }

    fn jni_call_return_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).jni_type.to_string(),
            Type::Void => "void".to_string(),
            _ => "jobject".to_string(),
        }
    }

    fn jni_call_method_suffix(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).call_suffix.to_string(),
            Type::Void => "Void".to_string(),
            _ => "Object".to_string(),
        }
    }

    fn c_type_for_callback(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.c_type_name().to_string(),
            Type::Void => "void".to_string(),
            _ => "void*".to_string(),
        }
    }

    fn jni_type_for_callback(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).jni_type.to_string(),
            _ => "jobject".to_string(),
        }
    }

    fn jni_arg_for_callback(name: &str, ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p)
                .jni_cast
                .map(|cast| format!("{}{}", cast, name))
                .unwrap_or_else(|| name.to_string()),
            _ => name.to_string(),
        }
    }

    fn build_jni_signature(inputs: &[TraitMethodParam], returns: &ReturnType) -> String {
        let params_sig: String = std::iter::once("J".to_string())
            .chain(
                inputs
                    .iter()
                    .map(|p| Self::jni_type_signature(&p.param_type)),
            )
            .collect();

        let return_sig = returns
            .ok_type()
            .map(Self::jni_type_signature)
            .unwrap_or_else(|| "V".to_string());

        format!("({}){}", params_sig, return_sig)
    }

    fn jni_type_signature(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => primitives::info(*p).signature.to_string(),
            Type::Void => "V".to_string(),
            _ => "Ljava/nio/ByteBuffer;".to_string(),
        }
    }

    fn is_supported_async_function(func: &Function, module: &Module) -> bool {
        AsyncCallPlan::supports_call(&func.inputs, &func.returns, module)
    }

    fn map_async_function(
        func: &Function,
        jni_prefix: &str,
        module: &Module,
    ) -> JniAsyncFunctionView {
        let ffi_name = naming::function_ffi_name(&func.name);
        let jni_func_name = ffi_name.replace('_', "_1");

        let params: Vec<JniParamInfo> = func
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let return_abi = ReturnAbi::from_return_type(&func.returns, module);
        let complete_is_void = return_abi.is_unit();
        let complete_is_wire_encoded = return_abi.is_wire_encoded();

        JniAsyncFunctionView {
            ffi_name: ffi_name.clone(),
            ffi_poll: naming::function_ffi_poll(&func.name),
            ffi_complete: naming::function_ffi_complete(&func.name),
            ffi_cancel: naming::function_ffi_cancel(&func.name),
            ffi_free: naming::function_ffi_free(&func.name),
            jni_create_name: format!("Java_{}_Native_{}", jni_prefix, jni_func_name),
            jni_poll_name: format!("Java_{}_Native_{}_1poll", jni_prefix, jni_func_name),
            jni_complete_name: format!("Java_{}_Native_{}_1complete", jni_prefix, jni_func_name),
            jni_cancel_name: format!("Java_{}_Native_{}_1cancel", jni_prefix, jni_func_name),
            jni_free_name: format!("Java_{}_Native_{}_1free", jni_prefix, jni_func_name),
            jni_params: Self::format_jni_params(&params),
            jni_complete_return: return_abi.jni_return_type().to_string(),
            jni_complete_c_type: return_abi.jni_c_return_type().to_string(),
            complete_is_void,
            complete_is_string: false,
            complete_is_vec: false,
            complete_is_record: complete_is_wire_encoded,
            complete_is_result: false,
            vec_buf_type: String::new(),
            vec_free_fn: String::new(),
            vec_jni_array_type: String::new(),
            vec_new_array_fn: String::new(),
            vec_set_array_fn: String::new(),
            vec_jni_element_type: String::new(),
            record_c_type: String::new(),
            record_struct_size: 0,
            result_ok_is_void: false,
            result_ok_is_string: false,
            result_ok_c_type: String::new(),
            result_ok_jni_type: String::new(),
            result_err_is_string: false,
            result_err_struct_size: 0,
            params,
        }
    }

    fn is_primitive_return(func: &Function) -> bool {
        super::is_primitive_only(func)
    }

    fn is_supported_function(func: &Function, module: &Module) -> bool {
        let supported_output = match &func.returns {
            ReturnType::Void => true,
            ReturnType::Fallible { ok, .. } => Self::is_supported_result_ok(ok, module),
            ReturnType::Value(ty) => match ty {
                Type::Void => true,
                Type::Primitive(_) => true,
                Type::String => true,
                Type::Enum(_) => true,
                Type::Vec(inner) => match inner.as_ref() {
                    Type::Primitive(_) => true,
                    Type::Record(record_name) => Self::is_record_blittable(record_name, module),
                    _ => false,
                },
                Type::Option(inner) => Self::is_supported_option_inner(inner, module),
                _ => false,
            },
        };

        let supported_inputs = func.inputs.iter().all(|param| match &param.param_type {
            Type::Primitive(_) | Type::String | Type::Enum(_) => true,
            Type::Record(name) => Self::is_record_blittable(name, module),
            Type::Vec(inner) | Type::Slice(inner) => match inner.as_ref() {
                Type::Primitive(_) => true,
                Type::Record(record_name) => Self::is_record_blittable(record_name, module),
                _ => false,
            },
            _ => false,
        });

        supported_output && supported_inputs
    }

    fn is_supported_option_inner(inner: &Type, module: &Module) -> bool {
        match inner {
            Type::Primitive(_) | Type::String => true,
            Type::Record(name) => Self::is_record_blittable(name, module),
            Type::Enum(name) => module.enums.iter().any(|e| &e.name == name),
            Type::Vec(vec_inner) => match vec_inner.as_ref() {
                Type::Primitive(_) | Type::String => true,
                Type::Record(name) => Self::is_record_blittable(name, module),
                Type::Enum(name) => module
                    .enums
                    .iter()
                    .any(|e| &e.name == name && !e.is_data_enum()),
                _ => false,
            },
            _ => false,
        }
    }

    fn is_supported_result_ok(ok: &Type, module: &Module) -> bool {
        match ok {
            Type::Primitive(_) | Type::String | Type::Void => true,
            Type::Record(name) => Self::is_record_blittable(name, module),
            Type::Enum(name) => module.enums.iter().any(|e| &e.name == name),
            Type::Vec(inner) => match inner.as_ref() {
                Type::Primitive(_) => true,
                Type::Record(name) => Self::is_record_blittable(name, module),
                _ => false,
            },
            Type::Option(inner) => Self::is_supported_option_inner(inner, module),
            _ => false,
        }
    }

    fn is_record_blittable(record_name: &str, module: &Module) -> bool {
        module
            .records
            .iter()
            .find(|record| record.name == record_name)
            .map(|record| record.is_blittable())
            .unwrap_or(false)
    }

    fn is_supported_async_method(method: &Method, module: &Module) -> bool {
        method.is_async && AsyncCallPlan::supports_call(&method.inputs, &method.returns, module)
    }

    fn map_function(
        func: &Function,
        prefix: &str,
        jni_prefix: &str,
        module: &Module,
    ) -> JniFunctionView {
        let ffi_name = format!("{}_{}", prefix, func.name);
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let return_kind = JniReturnKind::from_returns(&func.returns, &func.name, module);
        let params: Vec<JniParamInfo> = func
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let jni_return = return_kind.jni_return_type().to_string();
        let jni_params = Self::format_jni_params(&params);
        let vec_return = VecReturnKind::from_returns(&func.returns, &func.name, module);
        let option_vec_return =
            OptionVecReturnKind::from_returns(&func.returns, &func.name, module);
        let is_data_enum_return = return_kind.is_data_enum() && !func.returns.is_fallible();
        let data_enum_return_name = return_kind.data_enum_name().unwrap_or_default().to_string();
        let data_enum_return_size = return_kind.data_enum_struct_size();

        JniFunctionView {
            ffi_name,
            jni_name,
            jni_return,
            jni_params,
            return_kind: return_kind.clone(),
            params,
            is_wire_encoded: false,
            is_vec: vec_return.is_primitive(),
            is_vec_record: vec_return.is_record(),
            is_data_enum_return,
            data_enum_return_name,
            data_enum_return_size,
            vec_buf_type: Self::extract_buf_type(&vec_return),
            vec_free_fn: Self::extract_free_fn(&vec_return),
            vec_c_type: Self::extract_c_type(&vec_return),
            vec_jni_array_type: Self::extract_jni_array_type(&vec_return),
            vec_new_array_fn: Self::extract_new_array_fn(&vec_return),
            vec_struct_size: Self::extract_struct_size(&vec_return),
            option_vec_buf_type: Self::extract_option_vec_buf_type(&option_vec_return),
            option_vec_free_fn: Self::extract_option_vec_free_fn(&option_vec_return),
            option_vec_c_type: Self::extract_option_vec_c_type(&option_vec_return),
            option_vec_jni_array_type: Self::extract_option_vec_jni_array_type(&option_vec_return),
            option_vec_new_array_fn: Self::extract_option_vec_new_array_fn(&option_vec_return),
            option_vec_struct_size: Self::extract_option_vec_struct_size(&option_vec_return),
            option: return_kind.option_view().cloned(),
            result: Self::extract_result_view(&func.returns, module, &func.name),
        }
    }

    fn extract_result_view(
        returns: &ReturnType,
        module: &Module,
        func_name: &str,
    ) -> Option<ResultView> {
        match returns {
            ReturnType::Fallible { ok, err } => {
                Some(ResultView::from_result(ok, err, module, func_name))
            }
            _ => None,
        }
    }

    fn format_jni_params(params: &[JniParamInfo]) -> String {
        if params.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                params
                    .iter()
                    .map(|param| param.jni_param_decl())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }

    fn extract_buf_type(vec_return: &VecReturnKind) -> String {
        match vec_return {
            VecReturnKind::Primitive(info) => info.buf_type.clone(),
            VecReturnKind::Record(info) => info.buf_type.clone(),
            VecReturnKind::None => String::new(),
        }
    }

    fn extract_free_fn(vec_return: &VecReturnKind) -> String {
        match vec_return {
            VecReturnKind::Primitive(info) => info.free_fn.clone(),
            VecReturnKind::Record(info) => info.free_fn.clone(),
            VecReturnKind::None => String::new(),
        }
    }

    fn extract_c_type(vec_return: &VecReturnKind) -> String {
        match vec_return {
            VecReturnKind::Primitive(info) => info.c_type.clone(),
            _ => String::new(),
        }
    }

    fn extract_jni_array_type(vec_return: &VecReturnKind) -> String {
        match vec_return {
            VecReturnKind::Primitive(info) => info.jni_array_type.clone(),
            _ => String::new(),
        }
    }

    fn extract_new_array_fn(vec_return: &VecReturnKind) -> String {
        match vec_return {
            VecReturnKind::Primitive(info) => info.new_array_fn.clone(),
            _ => String::new(),
        }
    }

    fn extract_struct_size(vec_return: &VecReturnKind) -> usize {
        match vec_return {
            VecReturnKind::Record(info) => info.struct_size,
            _ => 0,
        }
    }

    fn extract_option_vec_buf_type(vec_return: &OptionVecReturnKind) -> String {
        match vec_return {
            OptionVecReturnKind::Primitive(info) => info.buf_type.clone(),
            OptionVecReturnKind::Record(info) => info.buf_type.clone(),
            OptionVecReturnKind::VecString(info) => info.buf_type.clone(),
            OptionVecReturnKind::VecEnum(info) => info.buf_type.clone(),
            OptionVecReturnKind::None => String::new(),
        }
    }

    fn extract_option_vec_free_fn(vec_return: &OptionVecReturnKind) -> String {
        match vec_return {
            OptionVecReturnKind::Primitive(info) => info.free_fn.clone(),
            OptionVecReturnKind::Record(info) => info.free_fn.clone(),
            OptionVecReturnKind::VecString(info) => info.free_fn.clone(),
            OptionVecReturnKind::VecEnum(info) => info.free_fn.clone(),
            OptionVecReturnKind::None => String::new(),
        }
    }

    fn extract_option_vec_c_type(vec_return: &OptionVecReturnKind) -> String {
        match vec_return {
            OptionVecReturnKind::Primitive(info) => info.c_type.clone(),
            _ => String::new(),
        }
    }

    fn extract_option_vec_jni_array_type(vec_return: &OptionVecReturnKind) -> String {
        match vec_return {
            OptionVecReturnKind::Primitive(info) => info.jni_array_type.clone(),
            _ => String::new(),
        }
    }

    fn extract_option_vec_new_array_fn(vec_return: &OptionVecReturnKind) -> String {
        match vec_return {
            OptionVecReturnKind::Primitive(info) => info.new_array_fn.clone(),
            _ => String::new(),
        }
    }

    fn extract_option_vec_struct_size(vec_return: &OptionVecReturnKind) -> usize {
        match vec_return {
            OptionVecReturnKind::Record(info) => info.struct_size,
            _ => 0,
        }
    }

    fn map_class(class: &Class, _prefix: &str, jni_prefix: &str, module: &Module) -> JniClassView {
        let ffi_prefix = naming::class_ffi_prefix(&class.name);

        let ctors: Vec<String> = class
            .constructors
            .iter()
            .filter(|ctor| {
                ctor.inputs
                    .iter()
                    .all(|param| WireFunctionPlan::supports_param_type(&param.param_type, module))
            })
            .map(|ctor| {
                JniWireCtorTemplate::from_constructor(class, ctor, jni_prefix, module)
                    .render()
                    .expect("JNI ctor template render failed")
            })
            .collect();

        let wire_methods: Vec<String> = class
            .methods
            .iter()
            .filter(|method| {
                !method.is_async
                    && WireFunctionPlan::supports_call(&method.inputs, &method.returns, module)
            })
            .map(|method| {
                JniWireMethodTemplate::from_method(class, method, jni_prefix, module)
                    .render()
                    .expect("JNI method template render failed")
            })
            .collect();

        let async_methods: Vec<JniAsyncFunctionView> = class
            .methods
            .iter()
            .filter(|m| Self::is_supported_async_method(m, module))
            .map(|method| Self::map_async_method(&class.name, method, jni_prefix, module))
            .collect();

        JniClassView {
            ffi_prefix: ffi_prefix.clone(),
            jni_ffi_prefix: ffi_prefix.replace('_', "_1"),
            jni_prefix: jni_prefix.to_string(),
            ctors,
            wire_methods,
            async_methods,
        }
    }

    fn map_async_method(
        class_name: &str,
        method: &Method,
        jni_prefix: &str,
        module: &Module,
    ) -> JniAsyncFunctionView {
        let ffi_name = naming::method_ffi_name(class_name, &method.name);
        let jni_func_name = ffi_name.replace('_', "_1");

        let params: Vec<JniParamInfo> = method
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let return_abi = ReturnAbi::from_return_type(&method.returns, module);
        let complete_is_void = return_abi.is_unit();
        let complete_is_wire_encoded = return_abi.is_wire_encoded();

        JniAsyncFunctionView {
            ffi_name: ffi_name.clone(),
            ffi_poll: naming::method_ffi_poll(class_name, &method.name),
            ffi_complete: naming::method_ffi_complete(class_name, &method.name),
            ffi_cancel: naming::method_ffi_cancel(class_name, &method.name),
            ffi_free: naming::method_ffi_free(class_name, &method.name),
            jni_create_name: format!("Java_{}_Native_{}", jni_prefix, jni_func_name),
            jni_poll_name: format!("Java_{}_Native_{}_1poll", jni_prefix, jni_func_name),
            jni_complete_name: format!("Java_{}_Native_{}_1complete", jni_prefix, jni_func_name),
            jni_cancel_name: format!("Java_{}_Native_{}_1cancel", jni_prefix, jni_func_name),
            jni_free_name: format!("Java_{}_Native_{}_1free", jni_prefix, jni_func_name),
            jni_params: Self::format_jni_params(&params),
            jni_complete_return: return_abi.jni_return_type().to_string(),
            jni_complete_c_type: return_abi.jni_c_return_type().to_string(),
            complete_is_void,
            complete_is_string: false,
            complete_is_vec: false,
            complete_is_record: complete_is_wire_encoded,
            complete_is_result: false,
            vec_buf_type: String::new(),
            vec_free_fn: String::new(),
            vec_jni_array_type: String::new(),
            vec_new_array_fn: String::new(),
            vec_set_array_fn: String::new(),
            vec_jni_element_type: String::new(),
            record_c_type: String::new(),
            record_struct_size: 0,
            result_ok_is_void: false,
            result_ok_is_string: false,
            result_ok_c_type: String::new(),
            result_ok_jni_type: String::new(),
            result_err_is_string: false,
            result_err_struct_size: 0,
            params,
        }
    }

    fn collect_closure_trampolines(
        module: &Module,
        package_path: &str,
    ) -> Vec<ClosureTrampolineView> {
        let mut seen_signature_ids = HashSet::<String>::new();

        module
            .functions
            .iter()
            .flat_map(|function| function.inputs.iter())
            .map(|param| &param.param_type)
            .chain(
                module
                    .classes
                    .iter()
                    .flat_map(|class| class.methods.iter())
                    .flat_map(|method| method.inputs.iter())
                    .map(|param| &param.param_type),
            )
            .filter_map(|param_type| match param_type {
                Type::Closure(signature) => {
                    let signature_id = signature.signature_id();
                    seen_signature_ids.insert(signature_id).then_some(signature)
                }
                _ => None,
            })
            .map(|signature| Self::build_trampoline_view(signature, package_path))
            .collect()
    }

    fn build_trampoline_view(sig: &ClosureSignature, _package_path: &str) -> ClosureTrampolineView {
        let signature_id = sig.signature_id();
        let trampoline_name = format!("trampoline_{}", signature_id);
        let invoke_method_name = "invoke";

        let c_params: Vec<String> = sig
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                if matches!(ty, Type::Record(_) | Type::String) {
                    format!("const uint8_t* p{}_ptr, uintptr_t p{}_len", i, i)
                } else {
                    format!("{} p{}", Self::closure_param_c_type(ty), i)
                }
            })
            .collect();
        let c_params_str = if c_params.is_empty() {
            String::new()
        } else {
            format!(", {}", c_params.join(", "))
        };

        let jni_signature = Self::build_closure_jni_signature(sig);

        let jni_call_args: Vec<String> = sig
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| Self::closure_param_to_jni(ty, i))
            .collect();
        let jni_call_args_str = jni_call_args.join(", ");

        let record_params: Vec<ClosureRecordParam> = sig
            .params
            .iter()
            .enumerate()
            .filter_map(|(i, ty)| {
                if let Type::Record(name) = ty {
                    Some(ClosureRecordParam {
                        index: i,
                        c_type: NamingConvention::class_name(name),
                        size: format!("sizeof({})", NamingConvention::class_name(name)),
                    })
                } else {
                    None
                }
            })
            .collect();

        ClosureTrampolineView {
            trampoline_name,
            signature_id,
            c_params: c_params_str,
            jni_signature,
            jni_call_args: jni_call_args_str,
            invoke_method_name: invoke_method_name.to_string(),
            record_params,
        }
    }

    fn closure_param_c_type(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.c_type_name().to_string(),
            Type::Record(_) | Type::String => "const uint8_t*, uintptr_t".to_string(),
            _ => "void*".to_string(),
        }
    }

    fn closure_param_to_jni(ty: &Type, index: usize) -> String {
        match ty {
            Type::Primitive(Primitive::Bool) => format!("(jboolean)p{}", index),
            Type::Primitive(Primitive::I8) => format!("(jbyte)p{}", index),
            Type::Primitive(Primitive::I16) => format!("(jshort)p{}", index),
            Type::Primitive(Primitive::I32) => format!("(jint)p{}", index),
            Type::Primitive(Primitive::I64) => format!("(jlong)p{}", index),
            Type::Primitive(Primitive::U8) => format!("(jbyte)p{}", index),
            Type::Primitive(Primitive::U16) => format!("(jshort)p{}", index),
            Type::Primitive(Primitive::U32) => format!("(jint)p{}", index),
            Type::Primitive(Primitive::U64) => format!("(jlong)p{}", index),
            Type::Primitive(Primitive::F32) => format!("(jfloat)p{}", index),
            Type::Primitive(Primitive::F64) => format!("(jdouble)p{}", index),
            Type::Record(_) => format!("buf_p{}", index),
            _ => format!("(jlong)p{}", index),
        }
    }

    fn build_closure_jni_signature(sig: &ClosureSignature) -> String {
        let params: String = sig.params.iter().map(Self::type_to_jni_sig).collect();
        let ret = Self::type_to_jni_sig(&sig.returns);
        format!("({}){}", params, ret)
    }

    fn type_to_jni_sig(ty: &Type) -> String {
        match ty {
            Type::Void => "V".to_string(),
            Type::Primitive(Primitive::Bool) => "Z".to_string(),
            Type::Primitive(Primitive::I8) | Type::Primitive(Primitive::U8) => "B".to_string(),
            Type::Primitive(Primitive::I16) | Type::Primitive(Primitive::U16) => "S".to_string(),
            Type::Primitive(Primitive::I32) | Type::Primitive(Primitive::U32) => "I".to_string(),
            Type::Primitive(Primitive::I64) | Type::Primitive(Primitive::U64) => "J".to_string(),
            Type::Primitive(Primitive::F32) => "F".to_string(),
            Type::Primitive(Primitive::F64) => "D".to_string(),
            Type::String => "Ljava/lang/String;".to_string(),
            Type::Record(_) => "Ljava/nio/ByteBuffer;".to_string(),
            _ => "Ljava/lang/Object;".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Parameter;

    fn empty_module() -> Module {
        Module::new("test")
    }

    #[test]
    fn test_jni_wire_function_no_params() {
        let func = Function::new("get_data")
            .with_return(ReturnType::value(Type::String))
            .with_wire_encoded();

        let module = empty_module();
        let template = JniWireFunctionTemplate::from_function(&func, "com_example", &module);
        let rendered = template.render().unwrap();

        assert!(rendered.contains("JNIEXPORT jobject JNICALL"));
        assert!(rendered.contains("Java_com_example_Native_riff_1get_1data"));
        assert!(rendered.contains("FfiBuf_u8 _buf = riff_get_data()"));
        assert!(rendered.contains("NewDirectByteBuffer"));
    }

    #[test]
    fn test_jni_wire_function_with_string_param() {
        let func = Function::new("search")
            .with_param(Parameter::new("query", Type::String))
            .with_return(ReturnType::value(Type::String))
            .with_wire_encoded();

        let module = empty_module();
        let template = JniWireFunctionTemplate::from_function(&func, "com_example", &module);
        let rendered = template.render().unwrap();

        assert!(rendered.contains("jstring query"));
        assert!(rendered.contains("GetStringUTFChars"));
        assert!(rendered.contains("ReleaseStringUTFChars"));
        assert!(rendered.contains("_query_c"));
    }

    #[test]
    fn test_jni_wire_function_with_primitive_param() {
        let func = Function::new("get_item")
            .with_param(Parameter::new("id", Type::Primitive(Primitive::I32)))
            .with_return(ReturnType::value(Type::String))
            .with_wire_encoded();

        let module = empty_module();
        let template = JniWireFunctionTemplate::from_function(&func, "com_example", &module);
        let rendered = template.render().unwrap();

        assert!(rendered.contains("jint id"));
        assert!(rendered.contains("riff_get_item(id)"));
    }
}
