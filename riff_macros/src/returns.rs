use quote::quote;
use proc_macro2::Span;
use syn::{ReturnType, Type};

use crate::custom_types;

pub enum OptionReturnAbi {
    OutValue { inner: syn::Type },
    OutFfiString,
    Vec { inner: syn::Type },
}

pub enum ReturnKind {
    Unit,
    Primitive,
    String,
    ResultPrimitive { ok: syn::Type, err: syn::Type },
    ResultString { err: syn::Type },
    ResultUnit { err: syn::Type },
    Vec(syn::Type),
    Option(OptionReturnAbi),
    WireEncoded(syn::Type),
}

pub enum AsyncReturnAbi {
    Unit,
    Direct { rust_type: proc_macro2::TokenStream },
    WireEncoded { rust_type: proc_macro2::TokenStream },
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

fn is_string_like_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "String"),
        Type::Reference(reference) => match reference.elem.as_ref() {
            Type::Path(path) => path.path.segments.last().is_some_and(|s| s.ident == "str"),
            _ => false,
        },
        _ => false,
    }
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
                    && args.args.len() >= 2
                    && let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first()
                    && let Some(syn::GenericArgument::Type(err_ty)) = args.args.iter().nth(1)
                {
                    let ok_str = quote!(#ok_ty).to_string().replace(' ', "");
                    if ok_str == "String" || ok_str == "std::string::String" {
                        return ReturnKind::ResultString {
                            err: err_ty.clone(),
                        };
                    } else if ok_str == "()" {
                        return ReturnKind::ResultUnit {
                            err: err_ty.clone(),
                        };
                    } else {
                        return ReturnKind::ResultPrimitive {
                            ok: ok_ty.clone(),
                            err: err_ty.clone(),
                        };
                    }
                }
                if segment.ident == "Option"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    if let Some(vec_inner) = extract_vec_inner(inner_ty) {
                        return ReturnKind::Option(OptionReturnAbi::Vec { inner: vec_inner });
                    }

                    if is_string_like_type(inner_ty) {
                        return ReturnKind::Option(OptionReturnAbi::OutFfiString);
                    }

                    return ReturnKind::Option(OptionReturnAbi::OutValue {
                        inner: inner_ty.clone(),
                    });
                }
            }

            if is_primitive_type(&type_str) {
                ReturnKind::Primitive
            } else {
                ReturnKind::WireEncoded(ty.as_ref().clone())
            }
        }
    }
}

pub fn classify_async_return_abi(output: &ReturnType) -> AsyncReturnAbi {
    match output {
        ReturnType::Default => AsyncReturnAbi::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote!(#ty).to_string().replace(' ', "");

            if type_str == "()" {
                return AsyncReturnAbi::Unit;
            }

            if type_str == "String"
                || type_str == "std::string::String"
                || type_str.starts_with("Vec<")
                || type_str.starts_with("Option<")
                || type_str.starts_with("Result<")
            {
                return AsyncReturnAbi::WireEncoded {
                    rust_type: quote! { #ty },
                };
            }

            if is_primitive_type(&type_str) {
                AsyncReturnAbi::Direct {
                    rust_type: quote! { #ty },
                }
            } else {
                AsyncReturnAbi::WireEncoded {
                    rust_type: quote! { #ty },
                }
            }
        }
    }
}

pub fn get_async_ffi_return_type(abi: &AsyncReturnAbi) -> proc_macro2::TokenStream {
    match abi {
        AsyncReturnAbi::Unit => quote! { () },
        AsyncReturnAbi::Direct { rust_type } => quote! { #rust_type },
        AsyncReturnAbi::WireEncoded { .. } => quote! { ::riff::__private::FfiBuf<u8> },
    }
}

pub fn get_async_rust_return_type(abi: &AsyncReturnAbi) -> proc_macro2::TokenStream {
    match abi {
        AsyncReturnAbi::Unit => quote! { () },
        AsyncReturnAbi::Direct { rust_type } | AsyncReturnAbi::WireEncoded { rust_type } => {
            quote! { #rust_type }
        }
    }
}

pub fn get_async_complete_conversion(abi: &AsyncReturnAbi) -> proc_macro2::TokenStream {
    match abi {
        AsyncReturnAbi::Unit => quote! {
            if !out_status.is_null() { *out_status = ::riff::__private::FfiStatus::OK; }
            ()
        },
        AsyncReturnAbi::Direct { .. } => quote! {
            if !out_status.is_null() { *out_status = ::riff::__private::FfiStatus::OK; }
            result
        },
        AsyncReturnAbi::WireEncoded { rust_type } => {
            let registry = custom_types::registry_for_current_crate().ok();
            let rust_type: syn::Type = syn::parse2(rust_type.clone())
                .unwrap_or_else(|_| syn::parse_quote!(::core::ffi::c_void));
            let needs_custom = registry
                .as_ref()
                .is_some_and(|r| custom_types::contains_custom_types(&rust_type, r));

            if needs_custom {
                let registry = registry.expect("custom types registry missing");
                let wire_ty = custom_types::wire_type_for(&rust_type, &registry);
                let result_ident = syn::Ident::new("result", Span::call_site());
                let wire_value_ident = syn::Ident::new("__riff_wire_value", Span::call_site());
                let to_wire = custom_types::to_wire_expr_owned(&rust_type, &registry, &result_ident);
                quote! {
                    if !out_status.is_null() { *out_status = ::riff::__private::FfiStatus::OK; }
                    let #wire_value_ident: #wire_ty = { #to_wire };
                    ::riff::__private::FfiBuf::wire_encode(&#wire_value_ident)
                }
            } else {
                quote! {
                    if !out_status.is_null() { *out_status = ::riff::__private::FfiStatus::OK; }
                    ::riff::__private::FfiBuf::wire_encode(&result)
                }
            }
        }
    }
}

pub fn get_async_default_ffi_value(abi: &AsyncReturnAbi) -> proc_macro2::TokenStream {
    match abi {
        AsyncReturnAbi::Unit => quote! { () },
        AsyncReturnAbi::Direct { .. } => quote! { Default::default() },
        AsyncReturnAbi::WireEncoded { .. } => quote! { ::riff::__private::FfiBuf::default() },
    }
}
