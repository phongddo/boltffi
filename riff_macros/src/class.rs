use proc_macro::TokenStream;
use quote::quote;
use riff_ffi_rules::naming;
use syn::{FnArg, ReturnType, Type};

use crate::params::{transform_method_params, FfiParams};

pub fn ffi_class_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemImpl);

    let self_ty = match input.self_ty.as_ref() {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.clone()),
        _ => None,
    };

    let type_name = match self_ty {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(&input, "ffi_class requires a named type")
                .to_compile_error()
                .into();
        }
    };

    let type_name_str = type_name.to_string();
    let new_ident = syn::Ident::new(&naming::class_ffi_new(&type_name_str), type_name.span());
    let free_ident = syn::Ident::new(&naming::class_ffi_free(&type_name_str), type_name.span());

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
                    return generate_method_export(&type_name, &type_name_str, method);
                }
            }
            None
        })
        .collect();

    let expanded = quote! {
        #input

        #[unsafe(no_mangle)]
        pub extern "C" fn #new_ident() -> *mut #type_name {
            Box::into_raw(Box::new(#type_name::new()))
        }

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

fn generate_method_export(
    type_name: &syn::Ident,
    class_name: &str,
    method: &syn::ImplItemFn,
) -> Option<proc_macro2::TokenStream> {
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        &naming::method_ffi_name(class_name, &method_name.to_string()),
        method_name.span(),
    );

    let has_self = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, FnArg::Receiver(_)))
        .unwrap_or(false);

    if !has_self {
        return None;
    }

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(other_inputs);

    let fn_output = &method.sig.output;
    let has_conversions = !conversions.is_empty();
    let is_unit_return = matches!(fn_output, ReturnType::Default);

    let call_expr = quote! { (*handle).#method_name(#(#call_args),*) };

    let (body, return_type) = if is_unit_return {
        let b = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr;
                crate::FfiStatus::OK
            }
        } else {
            quote! {
                #call_expr;
                crate::FfiStatus::OK
            }
        };
        (b, quote! { -> crate::FfiStatus })
    } else {
        let b = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr
            }
        } else {
            call_expr
        };
        (b, quote! { #fn_output })
    };

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

    let subscribe_ident =
        syn::Ident::new(&naming::stream_ffi_subscribe(class_name, &stream_name), method_name.span());
    let pop_batch_ident =
        syn::Ident::new(&naming::stream_ffi_pop_batch(class_name, &stream_name), method_name.span());
    let wait_ident =
        syn::Ident::new(&naming::stream_ffi_wait(class_name, &stream_name), method_name.span());
    let poll_ident =
        syn::Ident::new(&naming::stream_ffi_poll(class_name, &stream_name), method_name.span());
    let unsubscribe_ident =
        syn::Ident::new(&naming::stream_ffi_unsubscribe(class_name, &stream_name), method_name.span());
    let free_ident =
        syn::Ident::new(&naming::stream_ffi_free(class_name, &stream_name), method_name.span());

    quote! {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #subscribe_ident(
            handle: *const #type_name,
        ) -> crate::SubscriptionHandle {
            if handle.is_null() {
                return std::ptr::null_mut();
            }
            let instance = unsafe { &*handle };
            let subscription = instance.#method_name();
            std::sync::Arc::into_raw(subscription) as crate::SubscriptionHandle
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #pop_batch_ident(
            subscription_handle: crate::SubscriptionHandle,
            output_ptr: *mut #item_type,
            output_capacity: usize,
        ) -> usize {
            if subscription_handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
                return 0;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
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
            subscription_handle: crate::SubscriptionHandle,
            timeout_milliseconds: u32,
        ) -> i32 {
            if subscription_handle.is_null() {
                return crate::WaitResult::Unsubscribed as i32;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            subscription.wait_for_events(timeout_milliseconds) as i32
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #poll_ident(
            subscription_handle: crate::SubscriptionHandle,
            callback_data: u64,
            callback: crate::StreamContinuationCallback,
        ) {
            if subscription_handle.is_null() {
                callback(callback_data, crate::StreamPollResult::Closed);
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            subscription.poll(callback_data, callback);
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #unsubscribe_ident(
            subscription_handle: crate::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const crate::EventSubscription<#item_type>)
            };
            subscription.unsubscribe();
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(
            subscription_handle: crate::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            drop(unsafe {
                std::sync::Arc::from_raw(
                    subscription_handle as *const crate::EventSubscription<#item_type>
                )
            });
        }
    }
}
