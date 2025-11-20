pub const FFI_PREFIX: &str = "riff";

pub mod naming {
    use super::FFI_PREFIX;

    pub fn ffi_function_name(module_prefix: &str, func_name: &str) -> String {
        format!("{}_{}", module_prefix, func_name)
    }

    pub fn ffi_prefix() -> &'static str {
        FFI_PREFIX
    }

    pub fn vec_len_suffix() -> &'static str {
        "_len"
    }

    pub fn vec_copy_into_suffix() -> &'static str {
        "_copy_into"
    }

    pub fn param_ptr_suffix() -> &'static str {
        "_ptr"
    }

    pub fn param_len_suffix() -> &'static str {
        "_len"
    }

    pub fn snake_to_camel(name: &str) -> String {
        let mut result = String::with_capacity(name.len());
        let mut capitalize_next = false;

        for ch in name.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }

        result
    }
}

pub mod c_types {
    pub fn primitive_to_c(rust_type: &str) -> &'static str {
        match rust_type {
            "bool" => "bool",
            "i8" => "int8_t",
            "u8" => "uint8_t",
            "i16" => "int16_t",
            "u16" => "uint16_t",
            "i32" => "int32_t",
            "u32" => "uint32_t",
            "i64" => "int64_t",
            "u64" => "uint64_t",
            "f32" => "float",
            "f64" => "double",
            "usize" => "uintptr_t",
            "isize" => "intptr_t",
            _ => "void*",
        }
    }

    pub fn string_c_type() -> &'static str {
        "FfiString"
    }

    pub fn status_c_type() -> &'static str {
        "FfiStatus"
    }

    pub fn size_c_type() -> &'static str {
        "uintptr_t"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamTransform {
    Direct,
    StringToPtr,
    SliceToPtr { mutable: bool },
    VecToPtr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnTransform {
    Direct,
    Status,
    StringOut,
    VecLenAndCopy,
    OptionOut,
    ResultOut,
}

pub mod transforms {
    use super::{ParamTransform, ReturnTransform};

    pub fn classify_param(type_str: &str) -> ParamTransform {
        let type_str = type_str.trim();
        
        if type_str == "&str" || type_str == "& str" {
            return ParamTransform::StringToPtr;
        }
        if type_str == "String" {
            return ParamTransform::StringToPtr;
        }
        if type_str.starts_with("&[") && type_str.ends_with("]") {
            return ParamTransform::SliceToPtr { mutable: false };
        }
        if type_str.starts_with("&mut [") && type_str.ends_with("]") {
            return ParamTransform::SliceToPtr { mutable: true };
        }
        if type_str.starts_with("Vec<") && type_str.ends_with(">") {
            return ParamTransform::VecToPtr;
        }
        
        ParamTransform::Direct
    }

    pub fn classify_return(type_str: &str) -> ReturnTransform {
        let type_str = type_str.trim();
        
        if type_str.is_empty() || type_str == "()" {
            return ReturnTransform::Status;
        }
        if type_str == "String" {
            return ReturnTransform::StringOut;
        }
        if type_str.starts_with("Vec<") && type_str.ends_with(">") {
            return ReturnTransform::VecLenAndCopy;
        }
        if type_str.starts_with("Option<") && type_str.ends_with(">") {
            return ReturnTransform::OptionOut;
        }
        if type_str.starts_with("Result<") {
            return ReturnTransform::ResultOut;
        }
        
        ReturnTransform::Direct
    }
}

pub mod signatures {
    use super::c_types;
    use super::naming;

    #[derive(Clone)]
    pub struct FfiParam {
        pub name: String,
        pub c_type: String,
    }

    #[derive(Clone)]
    pub struct FfiSignature {
        pub name: String,
        pub params: Vec<FfiParam>,
        pub return_type: String,
    }

    pub fn string_param(param_name: &str) -> Vec<FfiParam> {
        vec![
            FfiParam {
                name: format!("{}{}", param_name, naming::param_ptr_suffix()),
                c_type: "const uint8_t*".to_string(),
            },
            FfiParam {
                name: format!("{}{}", param_name, naming::param_len_suffix()),
                c_type: c_types::size_c_type().to_string(),
            },
        ]
    }

    pub fn slice_param(param_name: &str, inner_c_type: &str, mutable: bool) -> Vec<FfiParam> {
        let ptr_type = if mutable {
            format!("{}*", inner_c_type)
        } else {
            format!("const {}*", inner_c_type)
        };
        vec![
            FfiParam {
                name: format!("{}{}", param_name, naming::param_ptr_suffix()),
                c_type: ptr_type,
            },
            FfiParam {
                name: format!("{}{}", param_name, naming::param_len_suffix()),
                c_type: c_types::size_c_type().to_string(),
            },
        ]
    }

    pub fn vec_param(param_name: &str, inner_c_type: &str) -> Vec<FfiParam> {
        slice_param(param_name, inner_c_type, false)
    }

    pub fn vec_return_signatures(base_name: &str, inner_c_type: &str, input_params: &[FfiParam]) -> Vec<FfiSignature> {
        let len_name = format!("{}{}", base_name, naming::vec_len_suffix());
        let copy_name = format!("{}{}", base_name, naming::vec_copy_into_suffix());

        let mut copy_params: Vec<FfiParam> = input_params.to_vec();
        copy_params.push(FfiParam {
            name: "dst".to_string(),
            c_type: format!("{}*", inner_c_type),
        });
        copy_params.push(FfiParam {
            name: "dst_cap".to_string(),
            c_type: c_types::size_c_type().to_string(),
        });
        copy_params.push(FfiParam {
            name: "written".to_string(),
            c_type: format!("{}*", c_types::size_c_type()),
        });

        vec![
            FfiSignature {
                name: len_name,
                params: input_params.to_vec(),
                return_type: c_types::size_c_type().to_string(),
            },
            FfiSignature {
                name: copy_name,
                params: copy_params,
                return_type: c_types::status_c_type().to_string(),
            },
        ]
    }

    pub fn string_return_signature(base_name: &str, input_params: &[FfiParam]) -> FfiSignature {
        let mut params = input_params.to_vec();
        params.push(FfiParam {
            name: "out".to_string(),
            c_type: format!("{}*", c_types::string_c_type()),
        });

        FfiSignature {
            name: base_name.to_string(),
            params,
            return_type: c_types::status_c_type().to_string(),
        }
    }
}
