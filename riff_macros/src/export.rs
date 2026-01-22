use proc_macro::TokenStream;
use quote::quote;
use riff_ffi_rules::naming;
use syn::ItemFn;

use crate::custom_types;
use crate::params::{FfiParams, transform_params, transform_params_async};
use crate::returns::{
    OptionReturnAbi, ReturnKind, classify_async_return_abi, classify_return,
    get_async_complete_conversion, get_async_default_ffi_value, get_async_ffi_return_type,
    get_async_rust_return_type,
};
use crate::safety;

fn should_wire_encode(kind: &ReturnKind) -> bool {
    matches!(
        kind,
        ReturnKind::String
            | ReturnKind::Vec(_)
            | ReturnKind::Option(_)
            | ReturnKind::ResultString { .. }
            | ReturnKind::ResultPrimitive { .. }
            | ReturnKind::ResultUnit { .. }
    )
}

fn convert_to_wire_encoded(kind: ReturnKind) -> ReturnKind {
    match kind {
        ReturnKind::String => {
            let ty: syn::Type = syn::parse_quote!(String);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::Vec(inner) => {
            let ty: syn::Type = syn::parse_quote!(Vec<#inner>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::Option(abi) => {
            let inner_ty = match &abi {
                OptionReturnAbi::OutValue { inner } => inner.clone(),
                OptionReturnAbi::OutFfiString => syn::parse_quote!(String),
                OptionReturnAbi::Vec { inner } => syn::parse_quote!(Vec<#inner>),
            };
            let ty: syn::Type = syn::parse_quote!(Option<#inner_ty>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::ResultString { err } => {
            let ty: syn::Type = syn::parse_quote!(Result<String, #err>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::ResultPrimitive { ok, err } => {
            let ty: syn::Type = syn::parse_quote!(Result<#ok, #err>);
            ReturnKind::WireEncoded(ty)
        }
        ReturnKind::ResultUnit { err } => {
            let ty: syn::Type = syn::parse_quote!(Result<(), #err>);
            ReturnKind::WireEncoded(ty)
        }
        other => other,
    }
}

pub fn ffi_export_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);

    let violations = safety::scan_function(&input);
    if !violations.is_empty() {
        return TokenStream::from(safety::violations_to_compile_errors(&violations));
    }

    let custom_types = match custom_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };

    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let is_async = input.sig.asyncness.is_some();

    if is_async {
        return generate_async_export(&input, &custom_types);
    }

    let export_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_params(fn_inputs, &custom_types);

    let has_params = !ffi_params.is_empty();
    let has_conversions = !conversions.is_empty();

    let return_kind = classify_return(fn_output);
    let return_kind = if should_wire_encode(&return_kind) {
        convert_to_wire_encoded(return_kind)
    } else {
        return_kind
    };

    let expanded = match return_kind {
        ReturnKind::Unit => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*);
                    ::riff::__private::FfiStatus::OK
                }
            } else {
                quote! {
                    #fn_name(#(#call_args),*);
                    ::riff::__private::FfiStatus::OK
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*
                    ) -> ::riff::__private::FfiStatus {
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() -> ::riff::__private::FfiStatus {
                        #body
                    }
                }
            }
        }
        ReturnKind::Primitive => {
            let fn_output = &input.sig.output;
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*)
                }
            } else {
                quote! { #fn_name(#(#call_args),*) }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*
                    ) #fn_output {
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() #fn_output {
                        #body
                    }
                }
            }
        }
        ReturnKind::WireEncoded(inner_ty) => {
            let needs_custom = custom_types::contains_custom_types(&inner_ty, &custom_types);
            let result_ident = syn::Ident::new("result", fn_name.span());

            let encode_body = if needs_custom {
                let wire_ty = custom_types::wire_type_for(&inner_ty, &custom_types);
                let wire_value_ident = syn::Ident::new("__riff_wire_value", fn_name.span());
                let to_wire = custom_types::to_wire_expr_owned(&inner_ty, &custom_types, &result_ident);
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
                    let #wire_value_ident: #wire_ty = { #to_wire };
                    ::riff::__private::FfiBuf::wire_encode(&#wire_value_ident)
                }
            } else {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
                    ::riff::__private::FfiBuf::wire_encode(&#result_ident)
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*
                    ) -> ::riff::__private::FfiBuf<u8> {
                        #encode_body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() -> ::riff::__private::FfiBuf<u8> {
                        #encode_body
                    }
                }
            }
        }
        ReturnKind::String
        | ReturnKind::ResultString { .. }
        | ReturnKind::ResultPrimitive { .. }
        | ReturnKind::ResultUnit { .. }
        | ReturnKind::Vec(_)
        | ReturnKind::Option(_) => unreachable!("converted to WireEncoded"),
    };

    TokenStream::from(expanded)
}

fn generate_async_export(input: &ItemFn, custom_types: &custom_types::CustomTypeRegistry) -> TokenStream {
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let fn_block = &input.block;

    let base_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let entry_ident = syn::Ident::new(&base_name, fn_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), fn_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), fn_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), fn_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), fn_name.span());

    let params = transform_params_async(fn_inputs, custom_types);
    let return_abi = classify_async_return_abi(fn_output);

    let ffi_return_type = get_async_ffi_return_type(&return_abi);
    let rust_return_type = get_async_rust_return_type(&return_abi);
    let complete_conversion = get_async_complete_conversion(&return_abi);
    let default_value = get_async_default_ffi_value(&return_abi);

    let ffi_params = &params.ffi_params;
    let pre_spawn = &params.pre_spawn;
    let thread_setup = &params.thread_setup;
    let call_args = &params.call_args;
    let move_vars = &params.move_vars;

    let future_body = quote! {
        #(#thread_setup)*
        #fn_name(#(#call_args),*).await
    };

    let entry_fn = if ffi_params.is_empty() {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident() -> ::riff::__private::RustFutureHandle {
                ::riff::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident(#(#ffi_params),*) -> ::riff::__private::RustFutureHandle {
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                ::riff::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    let complete_fn = quote! {
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #complete_ident(
            handle: ::riff::__private::RustFutureHandle,
            out_status: *mut ::riff::__private::FfiStatus,
        ) -> #ffi_return_type {
            match ::riff::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => { #complete_conversion }
                None => {
                    if !out_status.is_null() { *out_status = ::riff::__private::FfiStatus::CANCELLED; }
                    #default_value
                }
            }
        }
    };

    let expanded = quote! {
        #fn_vis async fn #fn_name(#fn_inputs) #fn_output #fn_block

        #entry_fn

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #poll_ident(
            handle: ::riff::__private::RustFutureHandle,
            callback_data: u64,
            callback: ::riff::__private::RustFutureContinuationCallback,
        ) {
            unsafe { ::riff::__private::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data) }
        }

        #complete_fn

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #cancel_ident(handle: ::riff::__private::RustFutureHandle) {
            unsafe { ::riff::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle) }
        }

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #free_ident(handle: ::riff::__private::RustFutureHandle) {
            unsafe { ::riff::__private::rustfuture::rust_future_free::<#rust_return_type>(handle) }
        }
    };

    TokenStream::from(expanded)
}
