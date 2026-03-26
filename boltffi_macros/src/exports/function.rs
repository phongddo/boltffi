use boltffi_ffi_rules::naming;
use boltffi_ffi_rules::transport::EncodedReturnStrategy;
use proc_macro::TokenStream;
use quote::quote;
use syn::ItemFn;

use crate::callbacks::registry as callback_registry;
use crate::lowering::params::{FfiParams, transform_params, transform_params_async};
use crate::lowering::returns::lower::{encoded_return_body, encoded_return_buffer_expression};
use crate::lowering::returns::model::{
    DirectBufferReturnMethod, ResolvedReturn, ReturnInvocationContext, ReturnLoweringContext,
    ReturnPlatform, WasmOptionScalarEncoding,
};
use crate::registries::custom_types;
use crate::registries::data_types;
use crate::safety;

fn build_encoded_return_exports(
    input: &ItemFn,
    fn_vis: &syn::Visibility,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    resolved_return: &ResolvedReturn,
    encode_body: proc_macro2::TokenStream,
) -> TokenStream {
    let wasm_return_method = resolved_return
        .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm)
        .unwrap_or_else(|| {
            panic!(
                "encoded sync export must use a direct wasm buffer return carrier: {:?}",
                resolved_return.value_return_strategy()
            )
        });
    let native_return_method = resolved_return
        .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Native)
        .unwrap_or_else(|| {
            panic!(
                "encoded sync export must use a direct native buffer return carrier: {:?}",
                resolved_return.value_return_strategy()
            )
        });
    let wasm_return_type = direct_buffer_return_type(wasm_return_method);
    let wasm_return_body = direct_buffer_return_body(wasm_return_method, encode_body.clone());
    let native_return_type = direct_buffer_return_type(native_return_method);
    let native_return_body = direct_buffer_return_body(native_return_method, encode_body);

    let wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> #wasm_return_type {
                #wasm_return_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> #wasm_return_type {
                #wasm_return_body
            }
        },
    };

    let non_wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> #native_return_type {
                #native_return_body
            }
        },
        false => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> #native_return_type {
                #native_return_body
            }
        },
    };

    TokenStream::from(quote! {
        #input

        #wasm_export
        #non_wasm_export
    })
}

fn direct_buffer_return_type(return_method: DirectBufferReturnMethod) -> proc_macro2::TokenStream {
    match return_method {
        DirectBufferReturnMethod::Packed => quote! { u64 },
        DirectBufferReturnMethod::Descriptor => quote! { ::boltffi::__private::FfiBuf },
    }
}

fn direct_buffer_return_body(
    return_method: DirectBufferReturnMethod,
    encode_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match return_method {
        DirectBufferReturnMethod::Packed => quote! {
            let __boltffi_buf: ::boltffi::__private::FfiBuf = { #encode_body };
            __boltffi_buf.into_packed()
        },
        DirectBufferReturnMethod::Descriptor => encode_body,
    }
}

fn build_f64_wasm_return_exports(
    input: &ItemFn,
    fn_vis: &syn::Visibility,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> TokenStream {
    let wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> f64 {
                #wasm_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> f64 {
                #wasm_body
            }
        },
    };

    let non_wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
        false => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
    };

    TokenStream::from(quote! {
        #input

        #wasm_export
        #non_wasm_export
    })
}

fn build_void_wasm_return_exports(
    input: &ItemFn,
    fn_vis: &syn::Visibility,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> TokenStream {
    let wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() {
                #wasm_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) {
                #wasm_body
            }
        },
    };

    let non_wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
        false => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
    };

    TokenStream::from(quote! {
        #input

        #wasm_export
        #non_wasm_export
    })
}

fn ffi_export_item_impl(input: ItemFn) -> proc_macro2::TokenStream {
    let violations = safety::scan_function(&input);
    if !violations.is_empty() {
        return safety::violations_to_compile_errors(&violations);
    }

    let custom_types = match custom_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error(),
    };
    let callback_registry = match callback_registry::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error(),
    };
    let data_types = match data_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error(),
    };
    let return_lowering = ReturnLoweringContext::new(&custom_types, &data_types);

    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let is_async = input.sig.asyncness.is_some();

    if is_async {
        return generate_async_export(&input, &custom_types, &callback_registry).into();
    }

    let export_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let return_abi = return_lowering.lower_output(fn_output);
    let on_wire_record_error = return_abi.invalid_arg_early_return_statement();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_params(
        fn_inputs,
        &return_lowering,
        &callback_registry,
        &on_wire_record_error,
    );

    let has_params = !ffi_params.is_empty();

    if return_abi.is_unit() {
        let body = quote! {
            #(#conversions)*
            #fn_name(#(#call_args),*);
            ::boltffi::__private::FfiStatus::OK
        };

        if has_params {
            quote! {
                    #input

                    #[allow(clippy::not_unsafe_ptr_arg_deref)]
            #[unsafe(no_mangle)]
                    #fn_vis unsafe extern "C" fn #export_ident(
                        #(#ffi_params),*
                    ) -> ::boltffi::__private::FfiStatus {
                        #body
                    }
                }
        } else {
            quote! {
                    #input

                    #[allow(clippy::not_unsafe_ptr_arg_deref)]
            #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiStatus {
                        #body
                    }
                }
        }
    } else if return_abi.is_primitive_scalar() {
        let fn_output = &input.sig.output;
        let body = quote! {
            #(#conversions)*
            #fn_name(#(#call_args),*)
        };

        if has_params {
            quote! {
                    #input

                    #[allow(clippy::not_unsafe_ptr_arg_deref)]
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

                    #[allow(clippy::not_unsafe_ptr_arg_deref)]
            #[unsafe(no_mangle)]
                    #fn_vis extern "C" fn #export_ident() #fn_output {
                        #body
                    }
                }
        }
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let inner_ty = return_abi.rust_type();
        let result_ident = syn::Ident::new("result", fn_name.span());

        if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
            let option_value_ident = syn::Ident::new("value", fn_name.span());
            let option_scalar_encoding = WasmOptionScalarEncoding::from_option_rust_type(inner_ty)
                .expect("OptionScalar return must have a primitive Option inner type");
            let some_expression = option_scalar_encoding.some_expression(&option_value_ident);
            let call_and_bind = quote! {
                #(#conversions)*
                let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
            };

            let wasm_body = quote! {
                #call_and_bind
                match #result_ident {
                    Some(#option_value_ident) => #some_expression,
                    None => f64::NAN,
                }
            };

            let native_body = quote! {
                #call_and_bind
                ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
            };

            return build_f64_wasm_return_exports(
                &input,
                fn_vis,
                &export_ident,
                &ffi_params,
                wasm_body,
                native_body,
            )
            .into();
        }

        if matches!(strategy, EncodedReturnStrategy::DirectVec) {
            let call_and_bind = quote! {
                #(#conversions)*
                let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
            };

            let native_body = quote! {
                #call_and_bind
                <::boltffi::__private::Seal as ::boltffi::__private::VecTransport<_>>::pack(#result_ident)
            };

            let wasm_body = quote! {
                #call_and_bind
                let __buf = ::boltffi::__private::FfiBuf::from_vec(#result_ident);
                ::boltffi::__private::write_return_slot(__buf.as_ptr() as u32, __buf.len() as u32, __buf.cap() as u32, __buf.align() as u32);
                core::mem::forget(__buf);
            };

            return build_void_wasm_return_exports(
                &input,
                fn_vis,
                &export_ident,
                &ffi_params,
                wasm_body,
                native_body,
            )
            .into();
        }

        let encode_body = encoded_return_body(
            inner_ty,
            strategy,
            &result_ident,
            quote! { #fn_name(#(#call_args),*) },
            &conversions,
            &custom_types,
        );

        build_encoded_return_exports(
            &input,
            fn_vis,
            &export_ident,
            &ffi_params,
            &return_abi,
            encode_body,
        )
        .into()
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        let body = quote! {
            #(#conversions)*
            ::boltffi::__private::Passable::pack(#fn_name(#(#call_args),*))
        };

        let return_type = quote! { <#rust_type as ::boltffi::__private::Passable>::Out };

        if has_params {
            quote! {
                #input

                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #(#ffi_params),*
                ) -> #return_type {
                    #body
                }
            }
        } else {
            quote! {
            #input

            #[allow(clippy::not_unsafe_ptr_arg_deref)]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> #return_type {
                #body
            }
            }
        }
    } else {
        unreachable!(
            "unsupported function export return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    }
}

pub fn ffi_export_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    TokenStream::from(ffi_export_item_impl(input))
}

fn generate_async_export(
    input: &ItemFn,
    custom_types: &custom_types::CustomTypeRegistry,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> TokenStream {
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let fn_block = &input.block;

    let base_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let entry_ident = syn::Ident::new(&base_name, fn_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), fn_name.span());
    let poll_sync_ident = syn::Ident::new(&format!("{}_poll_sync", base_name), fn_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), fn_name.span());
    let panic_message_ident =
        syn::Ident::new(&format!("{}_panic_message", base_name), fn_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), fn_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), fn_name.span());
    let data_types = match data_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let return_lowering = ReturnLoweringContext::new(custom_types, &data_types);

    let on_wire_record_error = quote! { ::core::ptr::null() };
    let params = match transform_params_async(
        fn_inputs,
        &return_lowering,
        callback_registry,
        &on_wire_record_error,
    ) {
        Ok(params) => params,
        Err(error) => return error.to_compile_error().into(),
    };
    let return_abi = return_lowering.lower_output(fn_output);

    let ffi_return_type = return_abi.async_ffi_return_type();
    let rust_return_type = return_abi.async_rust_return_type();
    let complete_conversion = return_abi.async_complete_conversion(&return_lowering);
    let default_value = return_abi.async_default_ffi_value();

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
            #fn_vis extern "C" fn #entry_ident() -> ::boltffi::__private::RustFutureHandle {
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident(#(#ffi_params),*) -> ::boltffi::__private::RustFutureHandle {
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    let native_complete_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #complete_ident(
            handle: ::boltffi::__private::RustFutureHandle,
            out_status: *mut ::boltffi::__private::FfiStatus,
        ) -> #ffi_return_type {
            match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => { #complete_conversion }
                None => {
                    if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::CANCELLED; }
                    #default_value
                }
            }
        }
    };

    let wasm_complete_fn = if return_abi.is_primitive_scalar() {
        let rust_type = return_abi.rust_type();
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #complete_ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) -> #rust_type {
                match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Some(result) => result,
                    None => Default::default(),
                }
            }
        }
    } else if return_abi.is_unit() {
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #complete_ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) {
                let _ = ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle);
            }
        }
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let rust_type = return_abi.rust_type();
        let registry = custom_types::registry_for_current_crate().ok();
        let result_ident = syn::Ident::new("result", proc_macro2::Span::call_site());
        let encode_expression = if matches!(strategy, EncodedReturnStrategy::Utf8String) {
            quote! { ::boltffi::__private::FfiBuf::wire_encode(&#result_ident) }
        } else {
            encoded_return_buffer_expression(rust_type, strategy, &result_ident, registry.as_ref())
        };
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #complete_ident(
                out: *mut ::boltffi::__private::FfiBuf,
                handle: ::boltffi::__private::RustFutureHandle,
                _out_status: *mut ::boltffi::__private::FfiStatus,
            ) {
                if out.is_null() {
                    return;
                }
                let buf = match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Some(#result_ident) => { #encode_expression },
                    None => ::boltffi::__private::FfiBuf::empty(),
                };
                out.write(buf);
            }
        }
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #complete_ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) -> <#rust_type as ::boltffi::__private::Passable>::Out {
                match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Some(result) => ::boltffi::__private::Passable::pack(result),
                    None => Default::default(),
                }
            }
        }
    } else {
        unreachable!(
            "unsupported async function export return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    };

    let native_poll_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #poll_ident(
            handle: ::boltffi::__private::RustFutureHandle,
            callback_data: u64,
            callback: ::boltffi::__private::RustFutureContinuationCallback,
        ) {
            ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data)
        }
    };

    let wasm_poll_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #poll_sync_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> i32 {
            ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle)
        }
    };

    let wasm_panic_message_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #panic_message_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> ::boltffi::__private::FfiBuf {
            match ::boltffi::__private::rust_future_panic_message::<#rust_return_type>(handle) {
                Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                None => ::boltffi::__private::FfiBuf::empty(),
            }
        }
    };

    let expanded = quote! {
        #fn_vis async fn #fn_name(#fn_inputs) #fn_output #fn_block

        #entry_fn

        #native_poll_fn
        #wasm_poll_fn
        #wasm_panic_message_fn

        #native_complete_fn
        #wasm_complete_fn

        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #cancel_ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
        }

        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #free_ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
        }
    };

    TokenStream::from(expanded)
}
