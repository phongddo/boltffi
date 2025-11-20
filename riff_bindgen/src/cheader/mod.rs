use askama::Template;

use riff_ffi_rules::{c_types, naming::{self, snake_to_camel}, signatures::FfiParam};

use crate::model::{Function, Module, Parameter, Type};

#[derive(Template)]
#[template(path = "cheader/header.h", escape = "none")]
pub struct HeaderTemplate<'a> {
    prefix: &'a str,
    records: Vec<RecordView<'a>>,
    functions: Vec<FunctionView>,
}

struct RecordView<'a> {
    name: &'a str,
    fields: Vec<FieldView>,
}

struct FieldView {
    name: String,
    c_type: String,
}

struct FunctionView {
    signature: String,
}

pub struct CHeaderGenerator;

impl CHeaderGenerator {
    pub fn generate(module: &Module) -> String {
        let prefix = module.ffi_prefix();

        let records: Vec<RecordView> = module
            .records
            .iter()
            .map(|r| RecordView {
                name: &r.name,
                fields: r
                    .fields
                    .iter()
                    .map(|f| FieldView {
                        name: snake_to_camel(&f.name),
                        c_type: Self::type_to_c(&f.field_type),
                    })
                    .collect(),
            })
            .collect();

        let functions: Vec<FunctionView> = module
            .functions
            .iter()
            .flat_map(|f| Self::function_signatures(f, &prefix))
            .collect();

        let template = HeaderTemplate {
            prefix: &prefix,
            records,
            functions,
        };

        template.render().expect("Failed to render header template")
    }

    fn function_signatures(func: &Function, prefix: &str) -> Vec<FunctionView> {
        let ffi_name = naming::ffi_function_name(prefix, &func.name);
        let input_params = Self::build_params(&func.inputs);

        match &func.output {
            Some(Type::Vec(inner)) => {
                let inner_c = Self::type_to_c(inner);
                let sigs = riff_ffi_rules::signatures::vec_return_signatures(
                    &ffi_name,
                    &inner_c,
                    &input_params,
                );
                sigs.into_iter()
                    .map(|s| FunctionView {
                        signature: Self::format_signature(&s.name, &s.params, &s.return_type),
                    })
                    .collect()
            }
            Some(Type::String) => {
                let sig =
                    riff_ffi_rules::signatures::string_return_signature(&ffi_name, &input_params);
                vec![FunctionView {
                    signature: Self::format_signature(&sig.name, &sig.params, &sig.return_type),
                }]
            }
            _ => {
                let ret = Self::return_type_to_c(&func.output);
                vec![FunctionView {
                    signature: Self::format_signature(&ffi_name, &input_params, &ret),
                }]
            }
        }
    }

    fn build_params(inputs: &[Parameter]) -> Vec<FfiParam> {
        inputs
            .iter()
            .flat_map(|p| Self::param_to_ffi(p))
            .collect()
    }

    fn param_to_ffi(param: &Parameter) -> Vec<FfiParam> {
        match &param.param_type {
            Type::String => riff_ffi_rules::signatures::string_param(&param.name),
            Type::Vec(inner) => {
                let inner_c = Self::type_to_c(inner);
                riff_ffi_rules::signatures::vec_param(&param.name, &inner_c)
            }
            Type::Slice(inner) => {
                let inner_c = Self::type_to_c(inner);
                riff_ffi_rules::signatures::slice_param(&param.name, &inner_c, false)
            }
            Type::MutSlice(inner) => {
                let inner_c = Self::type_to_c(inner);
                riff_ffi_rules::signatures::slice_param(&param.name, &inner_c, true)
            }
            _ => vec![FfiParam {
                name: param.name.clone(),
                c_type: Self::type_to_c(&param.param_type),
            }],
        }
    }

    fn format_signature(name: &str, params: &[FfiParam], return_type: &str) -> String {
        let params_str = if params.is_empty() {
            "void".to_string()
        } else {
            params
                .iter()
                .map(|p| format!("{} {}", p.c_type, p.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!("{} {}({})", return_type, name, params_str)
    }

    fn return_type_to_c(output: &Option<Type>) -> String {
        match output {
            None | Some(Type::Void) => c_types::status_c_type().to_string(),
            Some(Type::Primitive(p)) => p.c_type_name().to_string(),
            Some(Type::Option(inner)) if inner.is_primitive() => "bool".to_string(),
            Some(Type::Result { .. }) => c_types::status_c_type().to_string(),
            _ => c_types::status_c_type().to_string(),
        }
    }

    fn type_to_c(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.c_type_name().to_string(),
            Type::String => c_types::string_c_type().to_string(),
            Type::Record(name) => name.clone(),
            Type::Enum(name) => name.clone(),
            Type::Object(_) => "void*".to_string(),
            Type::Vec(inner) => format!("{}*", Self::type_to_c(inner)),
            Type::Option(inner) => Self::type_to_c(inner),
            Type::Void => "void".to_string(),
            _ => "void*".to_string(),
        }
    }
}
