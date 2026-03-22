pub mod primitive;

pub const FFI_PREFIX: &str = "boltffi";

pub mod naming {
    use super::FFI_PREFIX;
    use std::fmt;
    use std::marker::PhantomData;

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct Name<K>(String, PhantomData<K>);

    impl<K> Name<K> {
        pub fn new(value: String) -> Self {
            Self(value, PhantomData)
        }

        pub fn as_str(&self) -> &str {
            self.0.as_str()
        }

        pub fn into_string(self) -> String {
            self.0
        }
    }

    impl<K> fmt::Display for Name<K> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.as_str())
        }
    }

    impl<K> AsRef<str> for Name<K> {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl<K> From<Name<K>> for String {
        fn from(name: Name<K>) -> Self {
            name.0
        }
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct GlobalSymbol;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct VtableField;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct VtableType;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct RegisterFn;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct CreateFn;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct ForeignType;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct ClassPrefix;

    const C_KEYWORDS: &[&str] = &[
        "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
        "enum", "extern", "float", "for", "goto", "if", "int", "long", "register", "return",
        "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned",
        "void", "volatile", "while",
    ];

    pub fn escape_c_keyword(name: &str) -> String {
        if C_KEYWORDS.contains(&name) {
            format!("{}_", name)
        } else {
            name.to_string()
        }
    }

    pub fn ffi_prefix() -> &'static str {
        FFI_PREFIX
    }

    pub fn to_snake_case(name: &str) -> String {
        let mut result = String::with_capacity(name.len() + 4);
        for (i, ch) in name.chars().enumerate() {
            if ch.is_uppercase() {
                if i > 0 {
                    result.push('_');
                }
                result.push(ch.to_ascii_lowercase());
            } else {
                result.push(ch);
            }
        }
        result
    }

    pub fn to_upper_camel_case(name: &str) -> String {
        let mut result = String::with_capacity(name.len());
        let mut capitalize_next = true;
        for ch in name.chars() {
            if ch == '_' || ch == '-' {
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

    pub fn class_ffi_prefix(class_name: &str) -> Name<ClassPrefix> {
        Name::new(format!("{}_{}", FFI_PREFIX, to_snake_case(class_name)))
    }

    pub fn class_ffi_new(class_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_new", class_ffi_prefix(class_name)))
    }

    pub fn class_ffi_free(class_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", class_ffi_prefix(class_name)))
    }

    pub fn method_ffi_name(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}", class_ffi_prefix(class_name), method_name))
    }

    pub fn method_ffi_poll(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_poll", method_ffi_name(class_name, method_name)))
    }

    pub fn method_ffi_complete(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!(
            "{}_complete",
            method_ffi_name(class_name, method_name)
        ))
    }

    pub fn method_ffi_cancel(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!(
            "{}_cancel",
            method_ffi_name(class_name, method_name)
        ))
    }

    pub fn method_ffi_free(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", method_ffi_name(class_name, method_name)))
    }

    pub fn function_ffi_name(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}", FFI_PREFIX, func_name))
    }

    pub fn function_ffi_poll(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_poll", function_ffi_name(func_name)))
    }

    pub fn function_ffi_complete(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_complete", function_ffi_name(func_name)))
    }

    pub fn function_ffi_cancel(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_cancel", function_ffi_name(func_name)))
    }

    pub fn function_ffi_free(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", function_ffi_name(func_name)))
    }

    pub fn function_ffi_vec_len(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!(
            "{}{}",
            function_ffi_name(func_name),
            vec_len_suffix()
        ))
    }

    pub fn function_ffi_vec_copy_into(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!(
            "{}{}",
            function_ffi_name(func_name),
            vec_copy_into_suffix()
        ))
    }

    pub fn stream_ffi_subscribe(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        method_ffi_name(class_name, stream_name)
    }

    pub fn stream_ffi_pop_batch(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!(
            "{}_pop_batch",
            method_ffi_name(class_name, stream_name)
        ))
    }

    pub fn stream_ffi_wait(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_wait", method_ffi_name(class_name, stream_name)))
    }

    pub fn stream_ffi_poll(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_poll", method_ffi_name(class_name, stream_name)))
    }

    pub fn stream_ffi_unsubscribe(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!(
            "{}_unsubscribe",
            method_ffi_name(class_name, stream_name)
        ))
    }

    pub fn stream_ffi_free(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", method_ffi_name(class_name, stream_name)))
    }

    pub fn free_buf() -> Name<GlobalSymbol> {
        Name::new(format!("{}_free_buf", FFI_PREFIX))
    }

    pub fn atomic_u8_cas() -> Name<GlobalSymbol> {
        Name::new(format!("{}_atomic_u8_cas", FFI_PREFIX))
    }

    pub fn trait_ffi_free(trait_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}_free", FFI_PREFIX, to_snake_case(trait_name)))
    }

    pub fn callback_vtable_name(trait_name: &str) -> Name<VtableType> {
        Name::new(format!("{}VTable", trait_name))
    }

    pub fn callback_foreign_name(trait_name: &str) -> Name<ForeignType> {
        Name::new(format!("Foreign{}", trait_name))
    }

    pub fn callback_register_fn(trait_name: &str) -> Name<RegisterFn> {
        Name::new(format!(
            "{}_register_{}_vtable",
            FFI_PREFIX,
            to_snake_case(trait_name)
        ))
    }

    pub fn callback_create_fn(trait_name: &str) -> Name<CreateFn> {
        Name::new(format!(
            "{}_create_{}_handle",
            FFI_PREFIX,
            to_snake_case(trait_name)
        ))
    }

    pub fn vtable_field_name(method_name: &str) -> Name<VtableField> {
        Name::new(to_snake_case(method_name))
    }

    pub fn module_name(crate_name: &str) -> String {
        to_upper_camel_case(crate_name)
    }

    pub fn ffi_module_name(crate_name: &str) -> String {
        format!("{}FFI", module_name(crate_name))
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

    #[deprecated(note = "use function_ffi_name instead")]
    pub fn ffi_function_name(module_prefix: &str, func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}", module_prefix, func_name))
    }
}

pub mod c_types {
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

    pub fn vec_return_signatures(
        base_name: &str,
        inner_c_type: &str,
        input_params: &[FfiParam],
    ) -> Vec<FfiSignature> {
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

pub mod callback {
    use super::naming::to_snake_case;
    use super::primitive::Primitive;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TypeId {
        Void,
        Primitive(Primitive),
        String,
        Bytes,
        Named(std::string::String),
    }

    impl TypeId {
        pub fn from_rust_type_str(s: &str) -> Self {
            if let Ok(primitive) = s.parse::<Primitive>() {
                return Self::Primitive(primitive);
            }
            match s {
                "String" | "&str" => Self::String,
                "()" => Self::Void,
                other => Self::Named(other.to_string()),
            }
        }

        pub fn as_signature_part(&self) -> std::string::String {
            match self {
                Self::Void => "Void".into(),
                Self::Primitive(primitive) => primitive.type_id().into(),
                Self::String => "String".into(),
                Self::Bytes => "Bytes".into(),
                Self::Named(name) => name.clone(),
            }
        }
    }

    pub fn closure_signature_id(params: &[TypeId], returns: &TypeId) -> String {
        let params_part = params
            .iter()
            .map(|p| p.as_signature_part())
            .collect::<Vec<_>>()
            .join("_");

        let is_void_return = matches!(returns, TypeId::Void);
        let ret_part = returns.as_signature_part();

        if is_void_return {
            if params_part.is_empty() {
                "Void".to_string()
            } else {
                params_part
            }
        } else if params_part.is_empty() {
            format!("To{}", ret_part)
        } else {
            format!("{}To{}", params_part, ret_part)
        }
    }

    pub fn closure_callback_id(params: &[TypeId], returns: &TypeId) -> String {
        format!("__Closure_{}", closure_signature_id(params, returns))
    }

    pub fn closure_callback_id_snake(params: &[TypeId], returns: &TypeId) -> String {
        to_snake_case(&closure_callback_id(params, returns))
    }

    pub fn callback_wasm_import_call(callback_id_snake: &str) -> String {
        format!("__boltffi_callback_{}_call", callback_id_snake)
    }

    pub fn callback_wasm_import_free(callback_id_snake: &str) -> String {
        format!("__boltffi_callback_{}_free", callback_id_snake)
    }

    pub fn callback_wasm_import_clone(callback_id_snake: &str) -> String {
        format!("__boltffi_callback_{}_clone", callback_id_snake)
    }

    pub fn callback_create_handle_global() -> &'static str {
        "boltffi_create_callback_handle"
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn p(primitive: Primitive) -> TypeId {
            TypeId::Primitive(primitive)
        }

        #[test]
        fn type_id_from_rust_primitives() {
            assert_eq!(TypeId::from_rust_type_str("bool"), p(Primitive::Bool));
            assert_eq!(TypeId::from_rust_type_str("i8"), p(Primitive::I8));
            assert_eq!(TypeId::from_rust_type_str("u8"), p(Primitive::U8));
            assert_eq!(TypeId::from_rust_type_str("i16"), p(Primitive::I16));
            assert_eq!(TypeId::from_rust_type_str("u16"), p(Primitive::U16));
            assert_eq!(TypeId::from_rust_type_str("i32"), p(Primitive::I32));
            assert_eq!(TypeId::from_rust_type_str("u32"), p(Primitive::U32));
            assert_eq!(TypeId::from_rust_type_str("i64"), p(Primitive::I64));
            assert_eq!(TypeId::from_rust_type_str("u64"), p(Primitive::U64));
            assert_eq!(TypeId::from_rust_type_str("f32"), p(Primitive::F32));
            assert_eq!(TypeId::from_rust_type_str("f64"), p(Primitive::F64));
            assert_eq!(TypeId::from_rust_type_str("isize"), p(Primitive::ISize));
            assert_eq!(TypeId::from_rust_type_str("usize"), p(Primitive::USize));
        }

        #[test]
        fn type_id_from_rust_string_types() {
            assert_eq!(TypeId::from_rust_type_str("String"), TypeId::String);
            assert_eq!(TypeId::from_rust_type_str("&str"), TypeId::String);
        }

        #[test]
        fn type_id_from_rust_void() {
            assert_eq!(TypeId::from_rust_type_str("()"), TypeId::Void);
        }

        #[test]
        fn type_id_from_rust_custom() {
            assert_eq!(
                TypeId::from_rust_type_str("Point"),
                TypeId::Named("Point".into())
            );
            assert_eq!(
                TypeId::from_rust_type_str("MyCustomType"),
                TypeId::Named("MyCustomType".into())
            );
        }

        #[test]
        fn type_id_signature_parts() {
            assert_eq!(TypeId::Void.as_signature_part(), "Void");
            assert_eq!(p(Primitive::Bool).as_signature_part(), "Bool");
            assert_eq!(p(Primitive::I32).as_signature_part(), "I32");
            assert_eq!(TypeId::String.as_signature_part(), "String");
            assert_eq!(TypeId::Named("Point".into()).as_signature_part(), "Point");
        }

        #[test]
        fn closure_i32_to_i32() {
            let params = vec![p(Primitive::I32)];
            let returns = p(Primitive::I32);
            assert_eq!(closure_signature_id(&params, &returns), "I32ToI32");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_I32ToI32");
            assert_eq!(
                closure_callback_id_snake(&params, &returns),
                "___closure__i32_to_i32"
            );
        }

        #[test]
        fn closure_point_to_point() {
            let params = vec![TypeId::Named("Point".into())];
            let returns = TypeId::Named("Point".into());
            assert_eq!(closure_signature_id(&params, &returns), "PointToPoint");
            assert_eq!(
                closure_callback_id(&params, &returns),
                "__Closure_PointToPoint"
            );
        }

        #[test]
        fn closure_void_return() {
            let params = vec![p(Primitive::I32)];
            let returns = TypeId::Void;
            assert_eq!(closure_signature_id(&params, &returns), "I32");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_I32");
        }

        #[test]
        fn closure_no_params_with_return() {
            let params = vec![];
            let returns = p(Primitive::I32);
            assert_eq!(closure_signature_id(&params, &returns), "ToI32");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_ToI32");
        }

        #[test]
        fn closure_no_params_void_return() {
            let params = vec![];
            let returns = TypeId::Void;
            assert_eq!(closure_signature_id(&params, &returns), "Void");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_Void");
        }

        #[test]
        fn closure_multi_params() {
            let params = vec![p(Primitive::I32), TypeId::String];
            let returns = p(Primitive::Bool);
            assert_eq!(closure_signature_id(&params, &returns), "I32_StringToBool");
            assert_eq!(
                closure_callback_id(&params, &returns),
                "__Closure_I32_StringToBool"
            );
        }

        #[test]
        fn closure_all_primitives_void() {
            let params = vec![
                p(Primitive::Bool),
                p(Primitive::I8),
                p(Primitive::U8),
                p(Primitive::I16),
                p(Primitive::U16),
                p(Primitive::I32),
                p(Primitive::U32),
                p(Primitive::I64),
                p(Primitive::U64),
                p(Primitive::F32),
                p(Primitive::F64),
            ];
            let returns = TypeId::Void;
            let sig = closure_signature_id(&params, &returns);
            assert_eq!(sig, "Bool_I8_U8_I16_U16_I32_U32_I64_U64_F32_F64");
        }

        #[test]
        fn wasm_import_names() {
            let id_snake = "___closure__i32_to_i32";
            assert_eq!(
                callback_wasm_import_call(id_snake),
                "__boltffi_callback____closure__i32_to_i32_call"
            );
            assert_eq!(
                callback_wasm_import_free(id_snake),
                "__boltffi_callback____closure__i32_to_i32_free"
            );
            assert_eq!(
                callback_wasm_import_clone(id_snake),
                "__boltffi_callback____closure__i32_to_i32_clone"
            );
        }

        #[test]
        fn wasm_import_names_for_void_closure() {
            let params = vec![];
            let returns = TypeId::Void;
            let id_snake = closure_callback_id_snake(&params, &returns);
            assert_eq!(
                callback_wasm_import_call(&id_snake),
                "__boltffi_callback____closure__void_call"
            );
            assert_eq!(
                callback_wasm_import_free(&id_snake),
                "__boltffi_callback____closure__void_free"
            );
        }

        #[test]
        fn global_create_handle_name() {
            assert_eq!(
                callback_create_handle_global(),
                "boltffi_create_callback_handle"
            );
        }

        #[test]
        fn inv09_naming_deterministic() {
            let params = vec![p(Primitive::I32), TypeId::String];
            let returns = p(Primitive::Bool);

            let id1 = closure_callback_id_snake(&params, &returns);
            let id2 = closure_callback_id_snake(&params, &returns);
            assert_eq!(id1, id2, "INV-09: naming must be deterministic");

            let call1 = callback_wasm_import_call(&id1);
            let call2 = callback_wasm_import_call(&id2);
            assert_eq!(call1, call2, "INV-09: import names must be deterministic");
        }
    }
}

pub mod transport {
    //! When returning a buffer (wire-encoded data) from Rust to the host, we need
    //! to tell the host where the data lives and how big it is. Different platforms
    //! have different optimal ways to do this:
    //!
    //! - WASM: pointers are 32-bit, so ptr+len fits in a single u64 register.
    //!   Returning a packed u64 avoids allocating a separate descriptor struct.
    //!
    //! - Native 64-bit: pointers are 64-bit, so we can't pack ptr+len into u64.
    //!   We return a FfiBuf struct containing { ptr, len, cap }.
    //!
    //! This module defines the transport strategies so both the macro (which
    //! generates Rust FFI exports) and the codegen (which generates host bindings)
    //! use the same rules. Adding a new platform means adding a variant here and
    //! handling it in both macro and codegen.

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BufferTransport {
        /// Packs a buffer descriptor into a single integer return value.
        ///
        /// Example: WASM can return `ptr + len` as one packed `u64`.
        Packed,
        /// Returns a dedicated descriptor struct with pointer and length fields.
        ///
        /// Example: native 64-bit targets return an `FfiBuf`-style descriptor.
        Descriptor,
    }

    impl BufferTransport {
        pub fn for_target(target: &str) -> Self {
            match target {
                "wasm32" | "wasm32-unknown-unknown" | "wasm32-wasi" => Self::Packed,
                _ => Self::Descriptor,
            }
        }

        pub fn is_packed(self) -> bool {
            matches!(self, Self::Packed)
        }

        pub fn is_descriptor(self) -> bool {
            matches!(self, Self::Descriptor)
        }
    }

    /// Describes the encoded shape used when a return value is already crossing
    /// the boundary as bytes.
    ///
    /// # Examples
    ///
    /// - `String` uses [`Self::Utf8String`]
    /// - `Vec<u32>` on a wire path uses [`Self::DirectVec`]
    /// - `Option<u32>` can use [`Self::OptionScalar`]
    /// - `Result<u32, String>` can use [`Self::ResultScalar`]
    /// - nested records and enums use [`Self::WireEncoded`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum EncodedReturnStrategy {
        /// Returns a UTF-8 string buffer.
        ///
        /// The host receives bytes that already represent a UTF-8 string.
        ///
        /// Example: `fn greeting() -> String`
        Utf8String,
        /// Returns a vector whose elements are already laid out directly in the
        /// output buffer.
        ///
        /// "Direct" here means the buffer already contains the element ABI
        /// layout for that vector, not a filesystem directory and not a
        /// secondary wrapping format.
        ///
        /// Example: `fn numbers() -> Vec<u32>`
        DirectVec,
        /// Returns an `Option<T>` where the presence tag and scalar payload fit
        /// the compact scalar option layout.
        ///
        /// This is the compact path for simple scalar payloads such as
        /// `Option<u32>` or `Option<bool>`.
        ///
        /// Example: `fn maybe_count() -> Option<u32>`
        OptionScalar,
        /// Returns a `Result<T, E>` where the `Ok` payload uses the compact
        /// scalar result layout.
        ///
        /// This is the compact path for result values whose success payload can
        /// stay in the scalar result layout.
        ///
        /// Example: `fn parse_code() -> Result<u32, String>`
        ResultScalar,
        /// Returns a value through the general wire format.
        ///
        /// This is the fallback when the value does not fit one of the compact
        /// encoded layouts above.
        ///
        /// Example: `fn shape() -> Shape`
        WireEncoded,
    }

    /// Describes what kind of scalar the caller gets back.
    ///
    /// We need this because a plain integer and a c-style enum tag may share the
    /// same ABI type but they do not mean the same thing to host code.
    /// # Examples
    ///
    /// - `fn count() -> u32` uses [`Self::PrimitiveValue`]
    /// - `fn status() -> Status` where `Status` is a c-style enum uses
    ///   [`Self::CStyleEnumTag`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ScalarReturnStrategy {
        /// Returns a plain scalar value with no enum meaning attached to it.
        ///
        /// The caller should treat the bits as the scalar itself, not as an
        /// enum discriminant.
        ///
        /// Example: `fn count() -> u32`
        PrimitiveValue,
        /// Returns the raw discriminant of a c-style enum.
        ///
        /// The ABI value may still be an integer, but it represents an enum
        /// case tag rather than a free-standing numeric result.
        ///
        /// Example: `fn status() -> Status`
        CStyleEnumTag,
    }

    /// Describes the value itself that comes back across the boundary.
    ///
    /// This is about the returned value, not about the surrounding call shape.
    /// A function, method, callback, or inline closure can all use the same value
    /// return strategy even when they deliver that value through different ABI
    /// wiring.
    /// # Examples
    ///
    /// - `fn ping()` uses [`Self::Void`]
    /// - `fn count() -> u32` uses [`Self::Scalar`]
    /// - `fn point() -> Point` uses [`Self::CompositeValue`]
    /// - `fn counts() -> Vec<u32>` can use [`Self::DirectBuffer`]
    /// - `fn shape() -> Shape` can use [`Self::EncodedBuffer`]
    /// - `fn inventory() -> Inventory` uses [`Self::ObjectHandle`]
    /// - `fn callback() -> Box<dyn Mapper>` uses [`Self::CallbackHandle`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ValueReturnStrategy {
        /// Returns no value.
        ///
        /// The call exists for its side effects only.
        ///
        /// Example: `fn ping()`
        Void,
        /// Returns a scalar value directly.
        ///
        /// This keeps the value in its scalar ABI form instead of routing it
        /// through a buffer or handle.
        ///
        /// Example: `fn count() -> u32`
        Scalar(ScalarReturnStrategy),
        /// Returns a fixed composite value by value.
        ///
        /// The returned bits already match the ABI layout of the composite
        /// record or struct.
        ///
        /// Example: `fn point() -> Point`
        CompositeValue,
        /// Returns a sequence in its direct element layout instead of the
        /// general wire format.
        ///
        /// "Direct" here means the elements are exposed in their native ABI
        /// layout in the returned buffer. It does not mean a filesystem
        /// directory, and it does not mean the function must use the native
        /// return slot.
        ///
        /// Example: `fn counts() -> Vec<u32>`
        DirectBuffer,
        /// Returns bytes that need the general wire decoder to reconstruct the
        /// host value.
        ///
        /// This is the path for values that cannot stay in a simpler direct
        /// scalar, direct buffer, or fixed composite layout.
        ///
        /// Example: `fn shape() -> Shape`
        EncodedBuffer,
        /// Returns a foreign object handle.
        ///
        /// The caller receives an identity for an object that continues to live
        /// on the foreign side.
        ///
        /// Example: `fn inventory() -> Inventory`
        ObjectHandle,
        /// Returns a callback or trait-object handle.
        ///
        /// The caller receives a handle that can be used to invoke a callback
        /// surface later.
        ///
        /// Example: `fn callback() -> Box<dyn Mapper>`
        CallbackHandle,
    }

    /// Describes how failure is reported for a returned value.
    ///
    /// This is intentionally separate from [`ValueReturnStrategy`]. A call can
    /// return a primitive value, a handle, or an encoded buffer and still use a
    /// different error path.
    /// # Examples
    ///
    /// - `fn count() -> u32` uses [`Self::None`]
    /// - `fn inventory(capacity: u32) -> Result<Inventory, String>` can use
    ///   [`Self::StatusCode`]
    /// - `fn validate() -> Result<Point, ValidationError>` can use
    ///   [`Self::Encoded`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ErrorReturnStrategy {
        /// The call has no separate failure channel.
        ///
        /// Any failure would have to be expressed in the value itself, because
        /// the ABI surface does not reserve a distinct error path.
        ///
        /// Example: `fn count() -> u32`
        None,
        /// The call reports failure with a status code.
        ///
        /// The value path and the failure path are split: status tells the
        /// caller whether to read the value or fetch error details.
        ///
        /// Example: `fn inventory(capacity: u32) -> Result<Inventory, String>`
        StatusCode,
        /// The call reports failure with an encoded error payload.
        ///
        /// The error itself crosses the boundary as data rather than through a
        /// small status flag.
        ///
        /// Example: `fn validate() -> Result<Point, ValidationError>`
        Encoded,
    }

    /// Describes where the returned value is delivered in the ABI surface.
    ///
    /// Backends should not guess this from local templates. This tells them
    /// whether the value is carried as the native return value or written into
    /// output storage owned by the caller.
    /// # Examples
    ///
    /// - `fn count() -> u32` uses [`Self::DirectReturn`]
    /// - `fn point() -> Point` can use [`Self::WriteToOutParameter`]
    /// - `fn counts() -> Vec<u32>` with encoded errors can use
    ///   [`Self::WriteToOutBufferParts`]
    /// - a trampoline that writes into caller-owned scratch space can use
    ///   [`Self::WriteToReturnSlot`]
    /// - async completion handlers use [`Self::AsyncCallback`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ValueReturnMethod {
        /// Returns the value in the native function return position.
        ///
        /// The ABI return slot itself carries the result.
        ///
        /// Example: `fn count() -> u32`
        DirectReturn,
        /// Writes the value into a caller-provided out parameter.
        ///
        /// The caller allocates storage and passes a pointer where the callee
        /// writes the result.
        ///
        /// Example: `fn point(out: *mut Point)`
        WriteToOutParameter,
        /// Writes a buffer result through caller-provided pointer/length
        /// outputs.
        ///
        /// This is common when the returned bytes or element buffer need
        /// separate pointer and length outputs.
        ///
        /// Example: `fn counts(data_out: *mut *const u32, len_out: *mut usize)`
        WriteToOutBufferParts,
        /// Writes the value into a reserved caller-owned return slot.
        ///
        /// The caller already reserved result storage and the callee fills that
        /// storage in place.
        ///
        /// Example: callback vtable methods that receive a preallocated result
        /// slot for a composite return
        WriteToReturnSlot,
        /// Delivers the value through an async completion callback.
        ///
        /// The original call returns immediately, and the value arrives later
        /// through a completion boundary.
        ///
        /// Example: exported async functions that complete later
        AsyncCallback,
    }

    /// Describes how an encoded value is returned from a wasm sync export.
    ///
    /// This is narrower than [`ValueReturnMethod`]. It only answers the wasm
    /// calling convention for encoded sync export results after the value has
    /// already been classified as encoded.
    /// # Examples
    ///
    /// - `fn message() -> String` uses [`Self::PackedBuffer`]
    /// - `fn maybe_count() -> Option<u32>` can use [`Self::OptionalScalarF64`]
    /// - `fn counts() -> Vec<u32>` can use [`Self::ReturnSlot`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum WasmEncodedValueReturnMethod {
        /// Packs the encoded buffer into the wasm integer return value.
        ///
        /// The callee returns the packed buffer descriptor directly.
        ///
        /// Example: `fn message() -> String`
        PackedBuffer,
        /// Uses the scalar `f64` optional encoding reserved for wasm exports.
        ///
        /// This is the compact path for optional primitive scalars.
        ///
        /// Example: `fn maybe_count() -> Option<u32>`
        OptionalScalarF64,
        /// Writes the encoded buffer descriptor into the shared return slot.
        ///
        /// The function itself returns `()`, and the caller reads the buffer
        /// descriptor from reserved scratch space.
        ///
        /// Example: `fn counts() -> Vec<u32>`
        ReturnSlot,
    }

    /// Describes the sync export result strategy after Rust lowering has
    /// decided how the value must cross the ABI.
    ///
    /// This is the export-side counterpart to [`ValueReturnStrategy`]. It tells
    /// macro-generated exports whether the sync export returns a status, a
    /// scalar value, an encoded wasm-specific result, or a passable output.
    /// # Examples
    ///
    /// - `fn ping()` uses [`Self::Unit`]
    /// - `fn count() -> u32` uses [`Self::Scalar`]
    /// - `fn message() -> String` uses [`Self::Encoded`]
    /// - `fn point() -> Point` can use [`Self::Passable`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum SyncExportValueReturnStrategy {
        /// Returns only a status result.
        ///
        /// Example: `fn ping()`
        Unit,
        /// Returns a scalar value directly.
        ///
        /// Example: `fn count() -> u32`
        Scalar,
        /// Returns an encoded value using a wasm-specific encoded method.
        ///
        /// Example: `fn message() -> String`
        Encoded(WasmEncodedValueReturnMethod),
        /// Returns a passable value through its `Passable::Out` representation.
        ///
        /// Example: `fn point() -> Point`
        Passable,
    }

    /// Describes how Rust reconstructs the return value from a foreign callable
    /// such as a closure trampoline or callback bridge.
    ///
    /// This is about the bridge implementation, not the semantic meaning of the
    /// returned value.
    /// # Examples
    ///
    /// - a primitive callback result can use [`Self::PassableValue`]
    /// - a complex callback result can use [`Self::WireBufferValue`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ForeignCallableReturnMethod {
        /// Uses the `Passable` bridge for the return value.
        ///
        /// Example: a callback returning `u32`
        PassableValue,
        /// Uses a wire buffer and decodes the returned bytes afterward.
        ///
        /// Example: a callback returning `Shape`
        WireBufferValue,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn wasm_targets_use_packed() {
            assert_eq!(
                BufferTransport::for_target("wasm32"),
                BufferTransport::Packed
            );
            assert_eq!(
                BufferTransport::for_target("wasm32-unknown-unknown"),
                BufferTransport::Packed
            );
            assert_eq!(
                BufferTransport::for_target("wasm32-wasi"),
                BufferTransport::Packed
            );
        }

        #[test]
        fn native_targets_use_descriptor() {
            assert_eq!(
                BufferTransport::for_target("aarch64-apple-darwin"),
                BufferTransport::Descriptor
            );
            assert_eq!(
                BufferTransport::for_target("x86_64-unknown-linux-gnu"),
                BufferTransport::Descriptor
            );
            assert_eq!(
                BufferTransport::for_target("aarch64-linux-android"),
                BufferTransport::Descriptor
            );
        }

        #[test]
        fn scalar_return_strategy_distinguishes_enum_tags() {
            assert_ne!(
                ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue),
                ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag)
            );
        }

        #[test]
        fn value_return_method_distinguishes_direct_and_written_results() {
            assert_ne!(
                ValueReturnMethod::DirectReturn,
                ValueReturnMethod::WriteToOutBufferParts
            );
        }

        #[test]
        fn sync_export_value_return_strategy_distinguishes_encoded_paths() {
            assert_ne!(
                SyncExportValueReturnStrategy::Encoded(WasmEncodedValueReturnMethod::PackedBuffer),
                SyncExportValueReturnStrategy::Encoded(WasmEncodedValueReturnMethod::ReturnSlot)
            );
        }

        #[test]
        fn foreign_callable_return_method_distinguishes_passable_and_wire() {
            assert_ne!(
                ForeignCallableReturnMethod::PassableValue,
                ForeignCallableReturnMethod::WireBufferValue
            );
        }
    }
}

pub mod classification {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum PassableCategory {
        Scalar,
        Blittable,
        WireEncoded,
    }

    pub fn classify_struct(is_repr_c: bool, field_types: &[FieldPrimitive]) -> PassableCategory {
        if is_repr_c && !field_types.is_empty() && field_types.iter().all(|f| f.is_fixed_width) {
            PassableCategory::Blittable
        } else {
            PassableCategory::WireEncoded
        }
    }

    pub fn classify_enum(is_c_style: bool, has_integer_repr: bool) -> PassableCategory {
        if is_c_style && has_integer_repr {
            PassableCategory::Scalar
        } else {
            PassableCategory::WireEncoded
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct FieldPrimitive {
        pub is_fixed_width: bool,
    }

    impl FieldPrimitive {
        pub fn fixed() -> Self {
            Self {
                is_fixed_width: true,
            }
        }

        pub fn platform_sized() -> Self {
            Self {
                is_fixed_width: false,
            }
        }

        pub fn from_type_name(name: &str) -> Option<Self> {
            match name {
                "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64"
                | "bool" => Some(Self::fixed()),
                "isize" | "usize" => Some(Self::platform_sized()),
                _ => None,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn c_style_enum_with_repr_is_scalar() {
            assert_eq!(classify_enum(true, true), PassableCategory::Scalar);
        }

        #[test]
        fn data_enum_is_wire_encoded() {
            assert_eq!(classify_enum(false, true), PassableCategory::WireEncoded);
        }

        #[test]
        fn c_style_enum_without_repr_is_wire_encoded() {
            assert_eq!(classify_enum(true, false), PassableCategory::WireEncoded);
        }

        #[test]
        fn all_fixed_width_struct_is_blittable() {
            let fields = vec![FieldPrimitive::fixed(), FieldPrimitive::fixed()];
            assert_eq!(classify_struct(true, &fields), PassableCategory::Blittable);
        }

        #[test]
        fn struct_with_platform_sized_is_wire_encoded() {
            let fields = vec![FieldPrimitive::fixed(), FieldPrimitive::platform_sized()];
            assert_eq!(
                classify_struct(true, &fields),
                PassableCategory::WireEncoded
            );
        }

        #[test]
        fn struct_without_repr_c_is_wire_encoded() {
            let fields = vec![FieldPrimitive::fixed()];
            assert_eq!(
                classify_struct(false, &fields),
                PassableCategory::WireEncoded
            );
        }

        #[test]
        fn empty_struct_is_wire_encoded() {
            assert_eq!(classify_struct(true, &[]), PassableCategory::WireEncoded);
        }
    }
}
