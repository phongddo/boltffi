use proc_macro::TokenStream;
use quote::quote;
use riff_ffi_rules::naming;
use syn::ItemFn;

use crate::params::{transform_params, transform_params_async, FfiParams};
use crate::returns::{
    classify_async_return, classify_return, get_complete_conversion, get_default_ffi_value,
    get_ffi_return_type, get_rust_return_type, ReturnKind,
};

pub fn ffi_export_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let is_async = input.sig.asyncness.is_some();

    if is_async {
        return generate_async_export(&input);
    }

    let export_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_params(fn_inputs);

    let has_params = !ffi_params.is_empty();
    let has_conversions = !conversions.is_empty();

    let expanded = match classify_return(fn_output) {
        ReturnKind::String => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    let result = #fn_name(#(#call_args),*);
                    *out = crate::FfiString::from(result);
                    crate::FfiStatus::OK
                }
            } else {
                quote! {
                    let result = #fn_name(#(#call_args),*);
                    *out = crate::FfiString::from(result);
                    crate::FfiStatus::OK
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::ResultString => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = crate::FfiString::from(value);
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            } else {
                quote! {
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = crate::FfiString::from(value);
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut crate::FfiString
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::ResultPrimitive(inner_ty) => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = value;
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            } else {
                quote! {
                    match #fn_name(#(#call_args),*) {
                        Ok(value) => {
                            *out = value;
                            crate::FfiStatus::OK
                        }
                        Err(e) => crate::fail_with_error(crate::FfiStatus::INTERNAL_ERROR, &e.to_string())
                    }
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut #inner_ty
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut #inner_ty
                    ) -> crate::FfiStatus {
                        if out.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::Vec(inner_ty) => {
            let len_fn_name = syn::Ident::new(&format!("{}_len", export_name), fn_name.span());
            let copy_fn_name =
                syn::Ident::new(&format!("{}_copy_into", export_name), fn_name.span());

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
                    #fn_vis unsafe extern "C" fn #len_fn_name(
                        #(#ffi_params),*
                    ) -> usize {
                        #body.len()
                    }

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #copy_fn_name(
                        #(#ffi_params),*,
                        out: *mut #inner_ty,
                        capacity: usize,
                        written: *mut usize
                    ) -> crate::FfiStatus {
                        if out.is_null() || written.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        let result = #body;
                        let to_copy = result.len().min(capacity);
                        core::ptr::copy_nonoverlapping(result.as_ptr(), out, to_copy);
                        *written = to_copy;
                        crate::FfiStatus::OK
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #len_fn_name() -> usize {
                        #body.len()
                    }

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #copy_fn_name(
                        out: *mut #inner_ty,
                        capacity: usize,
                        written: *mut usize
                    ) -> crate::FfiStatus {
                        if out.is_null() || written.is_null() {
                            return crate::FfiStatus::NULL_POINTER;
                        }
                        let result = #body;
                        let to_copy = result.len().min(capacity);
                        core::ptr::copy_nonoverlapping(result.as_ptr(), out, to_copy);
                        *written = to_copy;
                        crate::FfiStatus::OK
                    }
                }
            }
        }
        ReturnKind::OptionPrimitive(inner_ty) => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    match #fn_name(#(#call_args),*) {
                        Some(v) => {
                            *out = v;
                            1
                        }
                        None => 0,
                    }
                }
            } else {
                quote! {
                    match #fn_name(#(#call_args),*) {
                        Some(v) => {
                            *out = v;
                            1
                        }
                        None => 0,
                    }
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*,
                        out: *mut #inner_ty
                    ) -> i32 {
                        if out.is_null() {
                            return 0;
                        }
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        out: *mut #inner_ty
                    ) -> i32 {
                        if out.is_null() {
                            return 0;
                        }
                        #body
                    }
                }
            }
        }
        ReturnKind::Unit => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*);
                    crate::FfiStatus::OK
                }
            } else {
                quote! {
                    #fn_name(#(#call_args),*);
                    crate::FfiStatus::OK
                }
            };

            if has_params {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*
                    ) -> crate::FfiStatus {
                        #body
                    }
                }
            } else {
                quote! {
                    #input

                    #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() -> crate::FfiStatus {
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
    };

    TokenStream::from(expanded)
}

fn generate_async_export(input: &ItemFn) -> TokenStream {
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

    let params = transform_params_async(fn_inputs);
    let return_kind = classify_async_return(fn_output);

    let ffi_return_type = get_ffi_return_type(&return_kind);
    let rust_return_type = get_rust_return_type(&return_kind);
    let complete_conversion = get_complete_conversion(&return_kind);
    let default_value = get_default_ffi_value(&return_kind);

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
            #fn_vis extern "C" fn #entry_ident() -> crate::RustFutureHandle {
                crate::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident(#(#ffi_params),*) -> crate::RustFutureHandle {
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                crate::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    let expanded = quote! {
        #fn_vis async fn #fn_name(#fn_inputs) #fn_output #fn_block

        #entry_fn

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #poll_ident(
            handle: crate::RustFutureHandle,
            callback_data: u64,
            callback: crate::RustFutureContinuationCallback,
        ) {
            unsafe { crate::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data) }
        }

        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #complete_ident(
            handle: crate::RustFutureHandle,
            out_status: *mut crate::FfiStatus,
        ) -> #ffi_return_type {
            match crate::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => { #complete_conversion }
                None => {
                    if !out_status.is_null() { *out_status = crate::FfiStatus::CANCELLED; }
                    #default_value
                }
            }
        }

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #cancel_ident(handle: crate::RustFutureHandle) {
            unsafe { crate::rustfuture::rust_future_cancel::<#rust_return_type>(handle) }
        }

        #[unsafe(no_mangle)]
        #fn_vis extern "C" fn #free_ident(handle: crate::RustFutureHandle) {
            unsafe { crate::rustfuture::rust_future_free::<#rust_return_type>(handle) }
        }
    };

    TokenStream::from(expanded)
}
