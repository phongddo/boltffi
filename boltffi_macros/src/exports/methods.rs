use super::common::{impl_type_name, is_factory_constructor, is_result_of_self_type_path};

use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{FnArg, ReturnType, Type};

use crate::callbacks::registry as callback_registry;
use crate::lowering::params::{FfiParams, transform_method_params, transform_method_params_async};
use crate::lowering::returns::lower::{encoded_return_body, encoded_return_buffer_expression};
use crate::lowering::returns::model::{
    DirectBufferReturnMethod, ResolvedReturn, ReturnInvocationContext, ReturnLoweringContext,
    ReturnPlatform, WasmOptionScalarEncoding,
};
use crate::registries::custom_types;
use crate::registries::data_types;
use boltffi_ffi_rules::transport::EncodedReturnStrategy;

fn has_mut_self_methods(input: &syn::ItemImpl) -> bool {
    input.items.iter().any(|item| {
        if let syn::ImplItem::Fn(method) = item
            && matches!(method.vis, syn::Visibility::Public(_))
            && !method.attrs.iter().any(|a| a.path().is_ident("skip"))
            && let Some(FnArg::Receiver(r)) = method.sig.inputs.first()
        {
            return r.mutability.is_some();
        }
        false
    })
}

fn parse_single_threaded_attr(attr: &TokenStream) -> bool {
    use syn::parse::Parser;
    let parser = syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated;
    parser
        .parse(attr.clone())
        .map(|args| {
            args.iter()
                .any(|id| id == "single_threaded" || id == "thread_unsafe")
        })
        .unwrap_or(false)
}

pub fn ffi_class_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let single_threaded = parse_single_threaded_attr(&attr);
    let input = syn::parse_macro_input!(item as syn::ItemImpl);

    if has_mut_self_methods(&input) && !single_threaded {
        return syn::Error::new_spanned(
            &input,
            "BoltFFI: `&mut self` methods are not thread-safe in FFI contexts\n\n\
             Two threads calling `&mut self` on the same instance = undefined behavior.\n\n\
             Options:\n\
             1. Use `&self` with interior mutability (Mutex, RwLock, atomics) [recommended]\n\
             2. Add #[export(single_threaded)] ONLY if you enforce thread safety in the target \
                language and want to avoid synchronization overhead you don't need",
        )
        .to_compile_error()
        .into();
    }

    let custom_types = match custom_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let callback_registry = match callback_registry::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let data_types = match data_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let return_lowering = ReturnLoweringContext::new(&custom_types, &data_types);

    let type_name = match impl_type_name(&input) {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(&input, "ffi_class requires a named type")
                .to_compile_error()
                .into();
        }
    };

    let type_name_str = type_name.to_string();
    let free_ident = syn::Ident::new(
        naming::class_ffi_free(&type_name_str).as_str(),
        type_name.span(),
    );

    let method_exports: Vec<_> = input
        .items
        .iter()
        .filter_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                if method.attrs.iter().any(|a| a.path().is_ident("skip")) {
                    return None;
                }
                if matches!(method.vis, syn::Visibility::Public(_)) {
                    if let Some(item_type) = extract_ffi_stream_item(&method.attrs) {
                        return Some(generate_stream_exports(
                            &type_name,
                            &type_name_str,
                            method,
                            &item_type,
                        ));
                    }
                    if method.sig.asyncness.is_some() {
                        return generate_async_method_export(
                            &type_name,
                            &type_name_str,
                            method,
                            &return_lowering,
                            &callback_registry,
                        );
                    }
                    return generate_method_export(
                        &type_name,
                        &type_name_str,
                        method,
                        &return_lowering,
                        &callback_registry,
                    );
                }
            }
            None
        })
        .collect();

    let thread_safety_assertion = if single_threaded {
        quote! {}
    } else {
        let span = type_name.span();
        quote_spanned! {span=>
            #[allow(dead_code)]
            const _: () = {
                #[diagnostic::on_unimplemented(
                    message = "BoltFFI: `{Self}` must be thread-safe (Send + Sync)",
                    note = "exported types can be accessed from any thread in the foreign language",
                    note = "add #[export(single_threaded)] if you guarantee single-threaded access"
                )]
                trait BoltFFIThreadSafe: Send + Sync {}
                impl<T: Send + Sync> BoltFFIThreadSafe for T {}
                fn _assert<T: BoltFFIThreadSafe>() {}
                fn _check() { _assert::<#type_name>(); }
            };
        }
    };

    let expanded = quote! {
        #input

        #thread_safety_assertion

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(handle: *mut #type_name) {
            if !handle.is_null() {
                drop(Box::from_raw(handle));
            }
        }

        #(#method_exports)*
    };

    TokenStream::from(expanded)
}

fn build_instance_encoded_return_exports(
    export_name: &syn::Ident,
    type_name: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    resolved_return: &ResolvedReturn,
    encode_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let wasm_return_method = resolved_return
        .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm)
        .unwrap_or_else(|| {
            panic!(
                "encoded instance sync export must use a direct wasm buffer return carrier: {:?}",
                resolved_return.value_return_strategy()
            )
        });
    let native_return_method = resolved_return
        .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Native)
        .unwrap_or_else(|| {
            panic!(
                "encoded instance sync export must use a direct native buffer return carrier: {:?}",
                resolved_return.value_return_strategy()
            )
        });
    let wasm_return_type = direct_buffer_return_type(wasm_return_method);
    let wasm_return_body = direct_buffer_return_body(wasm_return_method, encode_body.clone());
    let native_return_type = direct_buffer_return_type(native_return_method);
    let native_return_body = direct_buffer_return_body(native_return_method, encode_body);

    match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) -> #wasm_return_type {
                #wasm_return_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) -> #native_return_type {
                #native_return_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> #wasm_return_type {
                #wasm_return_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> #native_return_type {
                #native_return_body
            }
        },
    }
}

fn build_static_encoded_return_exports(
    export_name: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    resolved_return: &ResolvedReturn,
    encode_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let wasm_return_method = resolved_return
        .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm)
        .unwrap_or_else(|| {
            panic!(
                "encoded static sync export must use a direct wasm buffer return carrier: {:?}",
                resolved_return.value_return_strategy()
            )
        });
    let native_return_method = resolved_return
        .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Native)
        .unwrap_or_else(|| {
            panic!(
                "encoded static sync export must use a direct native buffer return carrier: {:?}",
                resolved_return.value_return_strategy()
            )
        });
    let wasm_return_type = direct_buffer_return_type(wasm_return_method);
    let wasm_return_body = direct_buffer_return_body(wasm_return_method, encode_body.clone());
    let native_return_type = direct_buffer_return_type(native_return_method);
    let native_return_body = direct_buffer_return_body(native_return_method, encode_body);

    match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> #wasm_return_type {
                #wasm_return_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> #native_return_type {
                #native_return_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> #wasm_return_type {
                #wasm_return_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> #native_return_type {
                #native_return_body
            }
        },
    }
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

fn build_instance_void_slot_exports(
    export_name: &syn::Ident,
    type_name: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
    }
}

fn build_static_void_slot_exports(
    export_name: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
    }
}

fn build_instance_f64_wasm_exports(
    export_name: &syn::Ident,
    type_name: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) -> f64 {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> f64 {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
    }
}

fn build_static_f64_wasm_exports(
    export_name: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> f64 {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> f64 {
                #wasm_body
            }

            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf {
                #native_body
            }
        },
    }
}

fn generate_factory_constructor_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = if method_name == "new" {
        naming::class_ffi_new(class_name)
    } else {
        naming::method_ffi_name(class_name, &method_name.to_string())
    };
    let export_name = syn::Ident::new(export_name.as_str(), method_name.span());

    let inputs = method.sig.inputs.iter().cloned();
    let on_wire_record_error = quote! { ::core::ptr::null_mut() };
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(
        inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    );

    let call_expr = quote! { #type_name::#method_name(#(#call_args),*) };

    let is_fallible = matches!(
        &method.sig.output,
        ReturnType::Type(_, ty)
            if matches!(ty.as_ref(), Type::Path(type_path) if is_result_of_self_type_path(&type_path.path, type_name))
    );

    let body = if is_fallible {
        let call = quote! { #call_expr };
        if conversions.is_empty() {
            quote! {
                match #call {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        } else {
            quote! {
                #(#conversions)*
                match #call {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        }
    } else if conversions.is_empty() {
        quote! { Box::into_raw(Box::new(#call_expr)) }
    } else {
        quote! {
            #(#conversions)*
            Box::into_raw(Box::new(#call_expr))
        }
    };

    if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> *mut #type_name {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> *mut #type_name {
                #body
            }
        })
    }
}

fn generate_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let custom_types = return_lowering.custom_types();
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        naming::method_ffi_name(class_name, &method_name.to_string()).as_str(),
        method_name.span(),
    );

    let has_self = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, FnArg::Receiver(_)))
        .unwrap_or(false);

    if !has_self {
        if is_factory_constructor(method, type_name) {
            return generate_factory_constructor_export(
                type_name,
                class_name,
                method,
                return_lowering,
                callback_registry,
            );
        }
        return generate_static_method_export(
            type_name,
            class_name,
            method,
            return_lowering,
            callback_registry,
        );
    }

    let return_abi = return_lowering.lower_output(&method.sig.output);
    let on_wire_record_error = return_abi.invalid_arg_early_return_statement();
    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(
        other_inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    );

    let has_conversions = !conversions.is_empty();

    let call_expr = quote! { (*handle).#method_name(#(#call_args),*) };

    let (body, return_type, is_wire_encoded) = if return_abi.is_unit() {
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        } else {
            quote! {
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        };
        (body, quote! { -> ::boltffi::__private::FfiStatus }, false)
    } else if return_abi.is_primitive_scalar() {
        let fn_output = &method.sig.output;
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr
            }
        } else {
            call_expr
        };
        (body, quote! { #fn_output }, false)
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let inner_ty = return_abi.rust_type();
        let result_ident = syn::Ident::new("result", method_name.span());

        if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
            let option_value_ident = syn::Ident::new("value", method_name.span());
            let option_scalar_encoding = WasmOptionScalarEncoding::from_option_rust_type(inner_ty)
                .expect("OptionScalar return must have a primitive Option inner type");
            let some_expression = option_scalar_encoding.some_expression(&option_value_ident);
            let call_and_bind = if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                }
            } else {
                quote! {
                    let #result_ident: #inner_ty = #call_expr;
                }
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

            return Some(build_instance_f64_wasm_exports(
                &export_name,
                type_name,
                &ffi_params,
                wasm_body,
                native_body,
            ));
        }

        if matches!(strategy, EncodedReturnStrategy::DirectVec) {
            let call_and_bind = if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                }
            } else {
                quote! {
                    let #result_ident: #inner_ty = #call_expr;
                }
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

            return Some(build_instance_void_slot_exports(
                &export_name,
                type_name,
                &ffi_params,
                wasm_body,
                native_body,
            ));
        }

        let body = encoded_return_body(
            inner_ty,
            strategy,
            &result_ident,
            quote! { #call_expr },
            &conversions,
            custom_types,
        );
        (body, quote! { -> ::boltffi::__private::FfiBuf }, true)
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        } else {
            quote! {
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        };
        let return_type = quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out };
        (body, return_type, false)
    } else {
        unreachable!(
            "unsupported instance method return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    };

    if is_wire_encoded {
        return Some(build_instance_encoded_return_exports(
            &export_name,
            type_name,
            &ffi_params,
            &return_abi,
            body,
        ));
    }

    if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name
            ) #return_type {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) #return_type {
                #body
            }
        })
    }
}

fn generate_static_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let custom_types = return_lowering.custom_types();
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        naming::method_ffi_name(class_name, &method_name.to_string()).as_str(),
        method_name.span(),
    );

    let return_abi = return_lowering.lower_output(&method.sig.output);
    let on_wire_record_error = return_abi.invalid_arg_early_return_statement();
    let all_inputs = method.sig.inputs.iter().cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(
        all_inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    );

    let has_conversions = !conversions.is_empty();
    let call_expr = quote! { #type_name::#method_name(#(#call_args),*) };

    let (body, return_type, is_wire_encoded) = if return_abi.is_unit() {
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        } else {
            quote! {
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        };
        (body, quote! { -> ::boltffi::__private::FfiStatus }, false)
    } else if return_abi.is_primitive_scalar() {
        let fn_output = &method.sig.output;
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr
            }
        } else {
            call_expr
        };
        (body, quote! { #fn_output }, false)
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let inner_ty = return_abi.rust_type();
        let result_ident = syn::Ident::new("result", method_name.span());

        if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
            let option_value_ident = syn::Ident::new("value", method_name.span());
            let option_scalar_encoding = WasmOptionScalarEncoding::from_option_rust_type(inner_ty)
                .expect("OptionScalar return must have a primitive Option inner type");
            let some_expression = option_scalar_encoding.some_expression(&option_value_ident);
            let call_and_bind = if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                }
            } else {
                quote! {
                    let #result_ident: #inner_ty = #call_expr;
                }
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

            return Some(build_static_f64_wasm_exports(
                &export_name,
                &ffi_params,
                wasm_body,
                native_body,
            ));
        }

        if matches!(strategy, EncodedReturnStrategy::DirectVec) {
            let call_and_bind = if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                }
            } else {
                quote! {
                    let #result_ident: #inner_ty = #call_expr;
                }
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

            return Some(build_static_void_slot_exports(
                &export_name,
                &ffi_params,
                wasm_body,
                native_body,
            ));
        }

        let body = encoded_return_body(
            inner_ty,
            strategy,
            &result_ident,
            quote! { #call_expr },
            &conversions,
            custom_types,
        );
        (body, quote! { -> ::boltffi::__private::FfiBuf }, true)
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        } else {
            quote! {
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        };
        let return_type = quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out };
        (body, return_type, false)
    } else {
        unreachable!(
            "unsupported static method return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    };

    if is_wire_encoded {
        return Some(build_static_encoded_return_exports(
            &export_name,
            &ffi_params,
            &return_abi,
            body,
        ));
    }

    if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name() #return_type {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(#(#ffi_params),*) #return_type {
                #body
            }
        })
    }
}

fn generate_async_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &callback_registry::CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let method_name_str = method_name.to_string();

    let receiver = match method.sig.inputs.first() {
        Some(FnArg::Receiver(r)) => r,
        _ => return None,
    };

    let needs_mut = receiver.mutability.is_some();

    let base_name = naming::method_ffi_name(class_name, &method_name_str);
    let entry_ident = syn::Ident::new(base_name.as_str(), method_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), method_name.span());
    let poll_sync_ident = syn::Ident::new(&format!("{}_poll_sync", base_name), method_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), method_name.span());
    let panic_message_ident =
        syn::Ident::new(&format!("{}_panic_message", base_name), method_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), method_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), method_name.span());

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let on_wire_record_error = quote! { ::core::ptr::null() };
    let params = match transform_method_params_async(
        other_inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    ) {
        Ok(params) => params,
        Err(error) => return Some(error.to_compile_error()),
    };

    let fn_output = &method.sig.output;
    let return_abi = return_lowering.lower_output(fn_output);

    let ffi_return_type = return_abi.async_ffi_return_type();
    let rust_return_type = return_abi.async_rust_return_type();
    let complete_conversion = return_abi.async_complete_conversion(return_lowering);
    let default_value = return_abi.async_default_ffi_value();

    let ffi_params = &params.ffi_params;
    let pre_spawn = &params.pre_spawn;
    let thread_setup = &params.thread_setup;
    let call_args = &params.call_args;
    let move_vars = &params.move_vars;

    let future_body = quote! {
        #(#thread_setup)*
        instance.#method_name(#(#call_args),*).await
    };

    let instance_binding = if needs_mut {
        quote! { let instance = &mut *handle; }
    } else {
        quote! { let instance = &*handle; }
    };

    let entry_fn = if ffi_params.is_empty() {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #entry_ident(
                handle: *mut #type_name
            ) -> ::boltffi::__private::RustFutureHandle {
                #instance_binding
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #entry_ident(
                handle: *mut #type_name,
                #(#ffi_params),*
            ) -> ::boltffi::__private::RustFutureHandle {
                #instance_binding
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
        pub unsafe extern "C" fn #complete_ident(
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

    let wasm_complete_fn = if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #complete_ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) -> <#rust_type as ::boltffi::__private::Passable>::Out {
                match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Some(result) => ::boltffi::__private::Passable::pack(result),
                    None => Default::default(),
                }
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
            pub unsafe extern "C" fn #complete_ident(
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
    } else {
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #complete_ident(
                out: *mut ::boltffi::__private::FfiBuf,
                handle: ::boltffi::__private::RustFutureHandle,
                _out_status: *mut ::boltffi::__private::FfiStatus,
            ) {
                if out.is_null() {
                    return;
                }
                let buf = match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Some(result) => ::boltffi::__private::FfiBuf::wire_encode(&result),
                    None => ::boltffi::__private::FfiBuf::empty(),
                };
                out.write(buf);
            }
        }
    };

    let native_poll_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #poll_ident(
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
        pub unsafe extern "C" fn #poll_sync_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> i32 {
            ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle)
        }
    };

    let wasm_panic_message_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #panic_message_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> ::boltffi::__private::FfiBuf {
            match ::boltffi::__private::rust_future_panic_message::<#rust_return_type>(handle) {
                Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                None => ::boltffi::__private::FfiBuf::empty(),
            }
        }
    };

    Some(quote! {
        #entry_fn

        #native_poll_fn
        #wasm_poll_fn
        #wasm_panic_message_fn

        #native_complete_fn
        #wasm_complete_fn

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #cancel_ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
        }
    })
}

fn extract_ffi_stream_item(attrs: &[syn::Attribute]) -> Option<syn::Type> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("ffi_stream") {
            return None;
        }

        let mut item_type: Option<syn::Type> = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("item") {
                let value: syn::Type = meta.value()?.parse()?;
                item_type = Some(value);
            }
            Ok(())
        });

        item_type
    })
}

fn generate_stream_exports(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
    item_type: &syn::Type,
) -> proc_macro2::TokenStream {
    let method_name = &method.sig.ident;
    let stream_name = method_name.to_string();

    let subscribe_ident = syn::Ident::new(
        naming::stream_ffi_subscribe(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let pop_batch_ident = syn::Ident::new(
        naming::stream_ffi_pop_batch(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let wait_ident = syn::Ident::new(
        naming::stream_ffi_wait(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let poll_ident = syn::Ident::new(
        naming::stream_ffi_poll(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let unsubscribe_ident = syn::Ident::new(
        naming::stream_ffi_unsubscribe(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let free_ident = syn::Ident::new(
        naming::stream_ffi_free(class_name, &stream_name).as_str(),
        method_name.span(),
    );

    quote! {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #subscribe_ident(
            handle: *const #type_name,
        ) -> ::boltffi::__private::SubscriptionHandle {
            if handle.is_null() {
                return std::ptr::null_mut();
            }
            let instance = unsafe { &*handle };
            let subscription = instance.#method_name();
            std::sync::Arc::into_raw(subscription) as ::boltffi::__private::SubscriptionHandle
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #pop_batch_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            output_ptr: *mut #item_type,
            output_capacity: usize,
        ) -> usize {
            if subscription_handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
                return 0;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            let output_slice = unsafe {
                std::slice::from_raw_parts_mut(
                    output_ptr as *mut std::mem::MaybeUninit<#item_type>,
                    output_capacity,
                )
            };
            subscription.pop_batch_into(output_slice)
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #wait_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            timeout_milliseconds: u32,
        ) -> i32 {
            if subscription_handle.is_null() {
                return ::boltffi::__private::WaitResult::Unsubscribed as i32;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.wait_for_events(timeout_milliseconds) as i32
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #poll_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            callback_data: u64,
            callback: ::boltffi::__private::StreamContinuationCallback,
        ) {
            if subscription_handle.is_null() {
                callback(callback_data, ::boltffi::__private::StreamPollResult::Closed);
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.poll(callback_data, callback);
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #unsubscribe_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.unsubscribe();
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            drop(unsafe {
                std::sync::Arc::from_raw(
                    subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_impl(code: &str) -> syn::ItemImpl {
        syn::parse_str(code).expect("failed to parse impl block")
    }

    fn extract_receiver_mutability(method: &syn::ImplItemFn) -> Option<bool> {
        match method.sig.inputs.first() {
            Some(FnArg::Receiver(r)) => Some(r.mutability.is_some()),
            _ => None,
        }
    }

    #[test]
    fn receiver_detection_ref_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn get(&self) -> i32 { self.value }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert_eq!(extract_receiver_mutability(method), Some(false));
    }

    #[test]
    fn receiver_detection_ref_mut_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn increment(&mut self) { self.value += 1; }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert_eq!(extract_receiver_mutability(method), Some(true));
    }

    #[test]
    fn receiver_detection_static_method() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn new(initial: i32) -> Self { Self { value: initial } }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert_eq!(extract_receiver_mutability(method), None);
    }

    #[test]
    fn async_method_ref_self_detected() {
        let impl_block = parse_impl(
            r#"
            impl Database {
                pub async fn query(&self, sql: String) -> String { sql }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert!(method.sig.asyncness.is_some());
        assert_eq!(extract_receiver_mutability(method), Some(false));
    }

    #[test]
    fn async_method_ref_mut_self_detected() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub async fn async_increment(&mut self) -> i32 { self.value += 1; self.value }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert!(method.sig.asyncness.is_some());
        assert_eq!(extract_receiver_mutability(method), Some(true));
    }

    #[test]
    fn instance_binding_generation_immutable() {
        let needs_mut = false;
        let instance_binding = if needs_mut {
            quote! { let instance = &mut *handle; }
        } else {
            quote! { let instance = &*handle; }
        };

        let output = instance_binding.to_string();
        assert!(output.contains("& * handle"));
        assert!(!output.contains("& mut"));
    }

    #[test]
    fn instance_binding_generation_mutable() {
        let needs_mut = true;
        let instance_binding = if needs_mut {
            quote! { let instance = &mut *handle; }
        } else {
            quote! { let instance = &*handle; }
        };

        let output = instance_binding.to_string();
        assert!(output.contains("& mut * handle"));
    }

    #[test]
    fn has_mut_self_methods_detects_mut_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn increment(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_ref_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn get(&self) -> i32 { self.value }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_static() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn new() -> Self { Counter { value: 0 } }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_private() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                fn private_mut(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_mixed_methods() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn new() -> Self { Counter { value: 0 } }
                pub fn get(&self) -> i32 { self.value }
                pub fn increment(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_skipped() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                #[skip]
                pub fn internal_mut(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }
}
