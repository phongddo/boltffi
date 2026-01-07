use askama::Template;
use riff_ffi_rules::naming;

use super::marshal::{JniParamInfo, JniReturnKind, OptionView, ResultView};
use crate::model::{Class, Function, Method, Module, Type};

#[derive(Template)]
#[template(path = "kotlin/jni_glue.txt", escape = "none")]
pub struct JniGlueTemplate {
    pub prefix: String,
    pub jni_prefix: String,
    pub package_path: String,
    pub module_name: String,
    pub functions: Vec<JniFunctionView>,
    pub classes: Vec<JniClassView>,
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
    fn from_output(output: &Option<Type>, _func_name: &str, module: &Module) -> Self {
        let Some(Type::Vec(inner)) = output else {
            return Self::None;
        };

        match inner.as_ref() {
            Type::Primitive(primitive) => {
                let cbindgen_name = primitive.cbindgen_name();
                Self::Primitive(PrimitiveVecInfo {
                    c_type: primitive.c_type_name().to_string(),
                    buf_type: format!("FfiBuf_{}", cbindgen_name),
                    free_fn: format!("riff_free_buf_{}", cbindgen_name),
                    jni_array_type: primitive.jni_array_type().to_string(),
                    new_array_fn: primitive.jni_new_array_fn().to_string(),
                })
            }
            Type::Record(record_name) => {
                let struct_size = module
                    .records
                    .iter()
                    .find(|record| &record.name == record_name)
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
    fn from_output(output: &Option<Type>, _func_name: &str, module: &Module) -> Self {
        let Some(Type::Option(inner)) = output else {
            return Self::None;
        };
        let Type::Vec(inner) = inner.as_ref() else {
            return Self::None;
        };

        match inner.as_ref() {
            Type::Primitive(primitive) => {
                let cbindgen_name = primitive.cbindgen_name();
                Self::Primitive(OptionPrimitiveVecInfo {
                    c_type: primitive.c_type_name().to_string(),
                    buf_type: format!("FfiBuf_{}", cbindgen_name),
                    free_fn: format!("riff_free_buf_{}", cbindgen_name),
                    jni_array_type: primitive.jni_array_type().to_string(),
                    new_array_fn: primitive.jni_new_array_fn().to_string(),
                })
            }
            Type::Record(record_name) => {
                let struct_size = module
                    .records
                    .iter()
                    .find(|record| &record.name == record_name)
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
                    .any(|e| &e.name == enum_name && e.is_data_enum());
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
    pub constructors: Vec<JniCtorView>,
    pub methods: Vec<JniMethodView>,
}

pub struct JniCtorView {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_params: String,
    pub params: Vec<JniParamInfo>,
}

pub struct JniMethodView {
    pub ffi_name: String,
    pub jni_name: String,
    pub jni_return: String,
    pub jni_params: String,
    pub return_kind: JniReturnKind,
    pub params: Vec<JniParamInfo>,
}

pub struct JniGenerator;

impl JniGenerator {
    pub fn generate(module: &Module, package: &str) -> String {
        let template = JniGlueTemplate::from_module(module, package);
        template.render().expect("JNI template render failed")
    }
}

impl JniGlueTemplate {
    pub fn from_module(module: &Module, package: &str) -> Self {
        let prefix = naming::ffi_prefix().to_string();
        let jni_prefix = package
            .replace('_', "_1")
            .replace('.', "_")
            .replace('-', "_1");
        let package_path = package.replace('.', "/");

        let functions: Vec<JniFunctionView> = module
            .functions
            .iter()
            .filter(|func| !func.is_async && Self::is_supported_function(func, module))
            .map(|func| Self::map_function(func, &prefix, &jni_prefix, module))
            .collect();

        let classes: Vec<JniClassView> = module
            .classes
            .iter()
            .map(|c| Self::map_class(c, &prefix, &jni_prefix))
            .collect();

        Self {
            prefix,
            jni_prefix,
            package_path,
            module_name: module.name.clone(),
            functions,
            classes,
        }
    }

    fn is_supported_function(func: &Function, module: &Module) -> bool {
        let supported_output = match &func.output {
            None => true,
            Some(Type::Primitive(_)) => true,
            Some(Type::String) => true,
            Some(Type::Enum(_)) => true,
            Some(Type::Vec(inner)) => match inner.as_ref() {
                Type::Primitive(_) => true,
                Type::Record(record_name) => Self::is_record_blittable(record_name, module),
                _ => false,
            },
            Some(Type::Option(inner)) => Self::is_supported_option_inner(inner, module),
            Some(Type::Result { ok, .. }) => Self::is_supported_result_ok(ok, module),
            _ => false,
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
                Type::Enum(name) => module.enums.iter().any(|e| &e.name == name && !e.is_data_enum()),
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

    fn is_supported_method(method: &Method) -> bool {
        let supported_output = match &method.output {
            None => true,
            Some(Type::Primitive(_)) => true,
            _ => false,
        };

        let supported_inputs = method
            .inputs
            .iter()
            .all(|p| matches!(&p.param_type, Type::Primitive(_)));

        supported_output && supported_inputs
    }

    fn map_function(
        func: &Function,
        prefix: &str,
        jni_prefix: &str,
        module: &Module,
    ) -> JniFunctionView {
        let ffi_name = format!("{}_{}", prefix, func.name);
        let jni_name = format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));

        let return_kind =
            JniReturnKind::from_type_with_module(func.output.as_ref(), &func.name, module);
        let params: Vec<JniParamInfo> = func
            .inputs
            .iter()
            .map(|param| {
                JniParamInfo::from_param_with_module(&param.name, &param.param_type, module)
            })
            .collect();

        let jni_return = return_kind.jni_return_type().to_string();
        let jni_params = Self::format_jni_params(&params);
        let vec_return = VecReturnKind::from_output(&func.output, &func.name, module);
        let option_vec_return = OptionVecReturnKind::from_output(&func.output, &func.name, module);
        let is_data_enum_return = return_kind.is_data_enum();
        let data_enum_return_name = return_kind
            .data_enum_name()
            .unwrap_or_default()
            .to_string();
        let data_enum_return_size = return_kind.data_enum_struct_size();

        JniFunctionView {
            ffi_name,
            jni_name,
            jni_return,
            jni_params,
            return_kind: return_kind.clone(),
            params,
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
            result: Self::extract_result_view(&func.output, module, &func.name),
        }
    }

    fn extract_result_view(
        output: &Option<Type>,
        module: &Module,
        func_name: &str,
    ) -> Option<ResultView> {
        match output {
            Some(Type::Result { ok, err }) => {
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

    fn map_class(class: &Class, _prefix: &str, jni_prefix: &str) -> JniClassView {
        let ffi_prefix = naming::class_ffi_prefix(&class.name);

        let constructors: Vec<JniCtorView> = class
            .constructors
            .iter()
            .map(|ctor| {
                let ffi_name = format!("{}_new", ffi_prefix);
                let jni_name = format!(
                    "Java_{}_Native_{}_1new",
                    jni_prefix,
                    ffi_prefix.replace('_', "_1")
                );
                let params: Vec<JniParamInfo> = ctor
                    .inputs
                    .iter()
                    .map(|p| JniParamInfo::from_param(&p.name, &p.param_type))
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
                JniCtorView {
                    ffi_name,
                    jni_name,
                    jni_params,
                    params,
                }
            })
            .collect();

        let methods: Vec<JniMethodView> = class
            .methods
            .iter()
            .filter(|m| Self::is_supported_method(m))
            .map(|method| {
                let ffi_name = naming::method_ffi_name(&class.name, &method.name);
                let jni_name =
                    format!("Java_{}_Native_{}", jni_prefix, ffi_name.replace('_', "_1"));
                let return_kind = JniReturnKind::from_type(method.output.as_ref(), &method.name);
                let params: Vec<JniParamInfo> = method
                    .inputs
                    .iter()
                    .map(|p| JniParamInfo::from_param(&p.name, &p.param_type))
                    .collect();
                let jni_return = return_kind.jni_return_type().to_string();
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
                JniMethodView {
                    ffi_name,
                    jni_name,
                    jni_return,
                    jni_params,
                    return_kind,
                    params,
                }
            })
            .collect();

        JniClassView {
            ffi_prefix: ffi_prefix.clone(),
            jni_ffi_prefix: ffi_prefix.replace('_', "_1"),
            jni_prefix: jni_prefix.to_string(),
            constructors,
            methods,
        }
    }
}
