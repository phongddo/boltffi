use quote::quote;
use syn::{ReturnType, Type};

pub enum ReturnKind {
    Unit,
    Primitive,
    String,
    ResultPrimitive(syn::Type),
    ResultString,
    Vec(syn::Type),
    OptionPrimitive(syn::Type),
}

pub enum AsyncReturnKind {
    Unit,
    Primitive(proc_macro2::TokenStream),
    String,
    Struct(proc_macro2::TokenStream),
    Vec(proc_macro2::TokenStream),
    Option(proc_macro2::TokenStream),
    ResultPrimitive(proc_macro2::TokenStream),
    ResultString,
    ResultStruct(proc_macro2::TokenStream),
    ResultVec(proc_macro2::TokenStream),
}

pub fn extract_vec_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn extract_generic_inner(ty: &Type, wrapper: &str) -> Option<syn::Type> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == wrapper
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn is_primitive_type(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "bool"
            | "usize"
            | "isize"
            | "()"
    )
}

pub fn classify_return(output: &ReturnType) -> ReturnKind {
    match output {
        ReturnType::Default => ReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote!(#ty).to_string().replace(' ', "");

            if type_str == "String" || type_str == "std::string::String" {
                return ReturnKind::String;
            }

            if let Some(inner) = extract_vec_inner(ty) {
                return ReturnKind::Vec(inner);
            }

            if let Type::Path(path) = ty.as_ref()
                && let Some(segment) = path.path.segments.last()
            {
                if segment.ident == "Result"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    let inner_str = quote!(#inner_ty).to_string().replace(' ', "");
                    if inner_str == "String" || inner_str == "std::string::String" {
                        return ReturnKind::ResultString;
                    } else if inner_str == "()" {
                        return ReturnKind::Unit;
                    } else {
                        return ReturnKind::ResultPrimitive(inner_ty.clone());
                    }
                }
                if segment.ident == "Option"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    return ReturnKind::OptionPrimitive(inner_ty.clone());
                }
            }

            ReturnKind::Primitive
        }
    }
}

pub fn classify_async_return(output: &ReturnType) -> AsyncReturnKind {
    match output {
        ReturnType::Default => AsyncReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote!(#ty).to_string().replace(' ', "");

            if type_str == "String" || type_str == "std::string::String" {
                return AsyncReturnKind::String;
            }

            if let Some(inner_ty) = extract_generic_inner(ty, "Vec") {
                return AsyncReturnKind::Vec(quote! { #inner_ty });
            }

            if let Some(inner_ty) = extract_generic_inner(ty, "Option") {
                return AsyncReturnKind::Option(quote! { #inner_ty });
            }

            if let Type::Path(path) = ty.as_ref()
                && let Some(segment) = path.path.segments.last()
                && segment.ident == "Result"
                && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
            {
                let inner_str = quote!(#inner_ty).to_string().replace(' ', "");

                if inner_str == "String" || inner_str == "std::string::String" {
                    return AsyncReturnKind::ResultString;
                } else if inner_str == "()" {
                    return AsyncReturnKind::Unit;
                } else if let Some(vec_inner) = extract_generic_inner(inner_ty, "Vec") {
                    return AsyncReturnKind::ResultVec(quote! { #vec_inner });
                } else if is_primitive_type(&inner_str) {
                    return AsyncReturnKind::ResultPrimitive(quote! { #inner_ty });
                } else {
                    return AsyncReturnKind::ResultStruct(quote! { #inner_ty });
                }
            }

            if is_primitive_type(&type_str) {
                AsyncReturnKind::Primitive(quote! { #ty })
            } else {
                AsyncReturnKind::Struct(quote! { #ty })
            }
        }
    }
}

pub fn get_ffi_return_type(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(ty) => quote! { #ty },
        AsyncReturnKind::String => quote! { crate::FfiString },
        AsyncReturnKind::Struct(ty) => quote! { #ty },
        AsyncReturnKind::Vec(inner_ty) => quote! { crate::FfiBuf<#inner_ty> },
        AsyncReturnKind::Option(inner_ty) => quote! { crate::FfiOption<#inner_ty> },
        AsyncReturnKind::ResultPrimitive(ty) => quote! { #ty },
        AsyncReturnKind::ResultString => quote! { crate::FfiString },
        AsyncReturnKind::ResultStruct(ty) => quote! { #ty },
        AsyncReturnKind::ResultVec(inner_ty) => quote! { crate::FfiBuf<#inner_ty> },
    }
}

pub fn get_rust_return_type(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(ty) => quote! { #ty },
        AsyncReturnKind::String => quote! { String },
        AsyncReturnKind::Struct(ty) => quote! { #ty },
        AsyncReturnKind::Vec(inner_ty) => quote! { Vec<#inner_ty> },
        AsyncReturnKind::Option(inner_ty) => quote! { Option<#inner_ty> },
        AsyncReturnKind::ResultPrimitive(ty) => {
            quote! { Result<#ty, Box<dyn std::error::Error + Send + Sync>> }
        }
        AsyncReturnKind::ResultString => {
            quote! { Result<String, Box<dyn std::error::Error + Send + Sync>> }
        }
        AsyncReturnKind::ResultStruct(ty) => {
            quote! { Result<#ty, Box<dyn std::error::Error + Send + Sync>> }
        }
        AsyncReturnKind::ResultVec(inner_ty) => {
            quote! { Result<Vec<#inner_ty>, Box<dyn std::error::Error + Send + Sync>> }
        }
    }
}

pub fn get_complete_conversion(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            ()
        },
        AsyncReturnKind::Primitive(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            result
        },
        AsyncReturnKind::String => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            crate::FfiString::from(result)
        },
        AsyncReturnKind::Struct(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            result
        },
        AsyncReturnKind::Vec(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            crate::FfiBuf::from_vec(result)
        },
        AsyncReturnKind::Option(_) => quote! {
            if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
            match result {
                Some(v) => crate::FfiOption { is_some: 1, value: v },
                None => crate::FfiOption { is_some: 0, value: Default::default() },
            }
        },
        AsyncReturnKind::ResultPrimitive(_) => quote! {
            match result {
                Ok(v) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    v
                }
                Err(e) => {
                    if !out_status.is_null() {
                        *out_status = crate::fail_with_error(
                            crate::FfiStatus::INTERNAL_ERROR,
                            &e.to_string()
                        );
                    }
                    Default::default()
                }
            }
        },
        AsyncReturnKind::ResultString => quote! {
            match result {
                Ok(v) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    crate::FfiString::from(v)
                }
                Err(e) => {
                    if !out_status.is_null() {
                        *out_status = crate::fail_with_error(
                            crate::FfiStatus::INTERNAL_ERROR,
                            &e.to_string()
                        );
                    }
                    crate::FfiString::default()
                }
            }
        },
        AsyncReturnKind::ResultStruct(_) => quote! {
            match result {
                Ok(v) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    v
                }
                Err(e) => {
                    if !out_status.is_null() {
                        *out_status = crate::fail_with_error(
                            crate::FfiStatus::INTERNAL_ERROR,
                            &e.to_string()
                        );
                    }
                    Default::default()
                }
            }
        },
        AsyncReturnKind::ResultVec(_) => quote! {
            match result {
                Ok(v) => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::OK; }
                    crate::FfiBuf::from_vec(v)
                }
                Err(e) => {
                    if !out_status.is_null() {
                        *out_status = crate::fail_with_error(
                            crate::FfiStatus::INTERNAL_ERROR,
                            &e.to_string()
                        );
                    }
                    crate::FfiBuf::default()
                }
            }
        },
    }
}

pub fn get_default_ffi_value(return_kind: &AsyncReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        AsyncReturnKind::Unit => quote! { () },
        AsyncReturnKind::Primitive(_) => quote! { Default::default() },
        AsyncReturnKind::String => quote! { crate::FfiString::default() },
        AsyncReturnKind::Struct(_) => quote! { Default::default() },
        AsyncReturnKind::Vec(_) => quote! { crate::FfiBuf::default() },
        AsyncReturnKind::Option(_) => {
            quote! { crate::FfiOption { is_some: 0, value: Default::default() } }
        }
        AsyncReturnKind::ResultPrimitive(_) => quote! { Default::default() },
        AsyncReturnKind::ResultString => quote! { crate::FfiString::default() },
        AsyncReturnKind::ResultStruct(_) => quote! { Default::default() },
        AsyncReturnKind::ResultVec(_) => quote! { crate::FfiBuf::default() },
    }
}
