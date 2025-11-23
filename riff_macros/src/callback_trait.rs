use proc_macro::TokenStream;
use quote::quote;
use riff_ffi_rules::naming;
use syn::{FnArg, Pat, ReturnType};

pub fn ffi_trait_impl(item: TokenStream) -> TokenStream {
    let item_trait = syn::parse_macro_input!(item as syn::ItemTrait);
    expand_ffi_trait(item_trait)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

fn expand_ffi_trait(item_trait: syn::ItemTrait) -> Result<proc_macro2::TokenStream, syn::Error> {
    let trait_name = &item_trait.ident;
    let trait_name_snake = to_snake_case_ident(&trait_name.to_string());
    let vtable_name = syn::Ident::new(&format!("{}VTable", trait_name), trait_name.span());
    let foreign_name = syn::Ident::new(&format!("Foreign{}", trait_name), trait_name.span());
    let vtable_static = syn::Ident::new(
        &format!("{}_VTABLE", trait_name_snake.to_string().to_uppercase()),
        trait_name.span(),
    );
    let register_fn = syn::Ident::new(
        &format!("{}_register_{}_vtable", naming::ffi_prefix(), trait_name_snake),
        trait_name.span(),
    );
    let create_fn = syn::Ident::new(
        &format!("{}_create_{}", naming::ffi_prefix(), trait_name_snake),
        trait_name.span(),
    );

    let mut vtable_fields = vec![
        quote! { pub free: extern "C" fn(handle: u64) },
        quote! { pub clone: extern "C" fn(handle: u64) -> u64 },
    ];

    let mut foreign_impls = Vec::new();

    for item in &item_trait.items {
        if let syn::TraitItem::Fn(method) = item {
            let method_name = &method.sig.ident;
            let method_name_snake = to_snake_case_ident(&method_name.to_string());
            let is_async = method.sig.asyncness.is_some();

            let mut param_types = Vec::new();
            let mut param_names = Vec::new();
            let mut call_args = Vec::new();

            for input in &method.sig.inputs {
                if let FnArg::Typed(pat_type) = input
                    && let Pat::Ident(pat_ident) = &*pat_type.pat
                {
                    let param_name = &pat_ident.ident;
                    let param_type = &pat_type.ty;

                    let ffi_type = rust_type_to_ffi_param_type(param_type);
                    param_types.push(quote! { #param_name: #ffi_type });
                    param_names.push(quote! { #param_name: #param_type });
                    call_args.push(quote! { #param_name });
                }
            }

            let return_type = match &method.sig.output {
                ReturnType::Default => None,
                ReturnType::Type(_, ty) => Some(ty.clone()),
            };

            if is_async {
                let callback_type = if let Some(ref ret_ty) = return_type {
                    let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
                    quote! { extern "C" fn(callback_data: u64, result: #ffi_ret, status: crate::FfiStatus) }
                } else {
                    quote! { extern "C" fn(callback_data: u64, status: crate::FfiStatus) }
                };

                vtable_fields.push(quote! {
                    pub #method_name_snake: extern "C" fn(
                        handle: u64,
                        #(#param_types,)*
                        callback: #callback_type,
                        callback_data: u64
                    )
                });

                let impl_body = if let Some(ref ret_ty) = return_type {
                    quote! {
                        use std::sync::Arc;
                        use std::sync::atomic::{AtomicBool, Ordering};

                        struct AsyncContext<T> {
                            result: std::cell::UnsafeCell<Option<T>>,
                            completed: AtomicBool,
                            waker: std::cell::UnsafeCell<Option<std::task::Waker>>,
                        }
                        unsafe impl<T> Send for AsyncContext<T> {}
                        unsafe impl<T> Sync for AsyncContext<T> {}

                        let ctx = Arc::new(AsyncContext::<#ret_ty> {
                            result: std::cell::UnsafeCell::new(None),
                            completed: AtomicBool::new(false),
                            waker: std::cell::UnsafeCell::new(None),
                        });

                        extern "C" fn callback<T: Copy>(data: u64, result: T, _status: crate::FfiStatus) {
                            let ctx = unsafe { Arc::from_raw(data as *const AsyncContext<T>) };
                            unsafe { *ctx.result.get() = Some(result) };
                            ctx.completed.store(true, Ordering::Release);
                            if let Some(waker) = unsafe { (*ctx.waker.get()).take() } {
                                waker.wake();
                            }
                        }

                        let ctx_ptr = Arc::into_raw(ctx.clone()) as u64;
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                callback::<#ret_ty>,
                                ctx_ptr
                            );
                        }

                        std::future::poll_fn(move |cx| {
                            if ctx.completed.load(Ordering::Acquire) {
                                let result = unsafe { (*ctx.result.get()).take().unwrap() };
                                std::task::Poll::Ready(result)
                            } else {
                                unsafe { *ctx.waker.get() = Some(cx.waker().clone()) };
                                std::task::Poll::Pending
                            }
                        }).await
                    }
                } else {
                    quote! {
                        use std::sync::Arc;
                        use std::sync::atomic::{AtomicBool, Ordering};

                        struct AsyncContext {
                            completed: AtomicBool,
                            waker: std::cell::UnsafeCell<Option<std::task::Waker>>,
                        }
                        unsafe impl Send for AsyncContext {}
                        unsafe impl Sync for AsyncContext {}

                        let ctx = Arc::new(AsyncContext {
                            completed: AtomicBool::new(false),
                            waker: std::cell::UnsafeCell::new(None),
                        });

                        extern "C" fn callback(data: u64, _status: crate::FfiStatus) {
                            let ctx = unsafe { Arc::from_raw(data as *const AsyncContext) };
                            ctx.completed.store(true, Ordering::Release);
                            if let Some(waker) = unsafe { (*ctx.waker.get()).take() } {
                                waker.wake();
                            }
                        }

                        let ctx_ptr = Arc::into_raw(ctx.clone()) as u64;
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                callback,
                                ctx_ptr
                            );
                        }

                        std::future::poll_fn(move |cx| {
                            if ctx.completed.load(Ordering::Acquire) {
                                std::task::Poll::Ready(())
                            } else {
                                unsafe { *ctx.waker.get() = Some(cx.waker().clone()) };
                                std::task::Poll::Pending
                            }
                        }).await
                    }
                };

                let output_type = return_type
                    .as_ref()
                    .map(|t| quote! { -> #t })
                    .unwrap_or_default();
                foreign_impls.push(quote! {
                    async fn #method_name(&self, #(#param_names,)*) #output_type {
                        #impl_body
                    }
                });
            } else {
                let out_param = if let Some(ref ret_ty) = return_type {
                    let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
                    quote! { out: *mut #ffi_ret, }
                } else {
                    quote! {}
                };

                vtable_fields.push(quote! {
                    pub #method_name_snake: extern "C" fn(
                        handle: u64,
                        #(#param_types,)*
                        #out_param
                        status: *mut crate::FfiStatus
                    )
                });

                let impl_body = if let Some(ref ret_ty) = return_type {
                    quote! {
                        let mut out: #ret_ty = Default::default();
                        let mut status = crate::FfiStatus::default();
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                &mut out as *mut _,
                                &mut status
                            );
                        }
                        out
                    }
                } else {
                    quote! {
                        let mut status = crate::FfiStatus::default();
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                &mut status
                            );
                        }
                    }
                };

                let output_type = return_type
                    .as_ref()
                    .map(|t| quote! { -> #t })
                    .unwrap_or_default();
                foreign_impls.push(quote! {
                    fn #method_name(&self, #(#param_names,)*) #output_type {
                        #impl_body
                    }
                });
            }
        }
    }

    let expanded = quote! {
        #item_trait

        #[repr(C)]
        pub struct #vtable_name {
            #(#vtable_fields),*
        }

        pub struct #foreign_name {
            vtable: *const #vtable_name,
            handle: u64,
        }

        unsafe impl Send for #foreign_name {}
        unsafe impl Sync for #foreign_name {}

        impl Drop for #foreign_name {
            fn drop(&mut self) {
                unsafe { ((*self.vtable).free)(self.handle) };
            }
        }

        impl Clone for #foreign_name {
            fn clone(&self) -> Self {
                let new_handle = unsafe { ((*self.vtable).clone)(self.handle) };
                Self {
                    vtable: self.vtable,
                    handle: new_handle,
                }
            }
        }

        impl #trait_name for #foreign_name {
            #(#foreign_impls)*
        }

        static #vtable_static: std::sync::atomic::AtomicPtr<#vtable_name> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

        #[unsafe(no_mangle)]
        pub extern "C" fn #register_fn(vtable: *const #vtable_name) {
            #vtable_static.store(vtable as *mut _, std::sync::atomic::Ordering::Release);
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn #create_fn(handle: u64) -> *mut #foreign_name {
            let vtable = #vtable_static.load(std::sync::atomic::Ordering::Acquire);
            if vtable.is_null() {
                return std::ptr::null_mut();
            }
            Box::into_raw(Box::new(#foreign_name { vtable, handle }))
        }
    };

    Ok(expanded)
}

fn to_snake_case_ident(name: &str) -> syn::Ident {
    syn::Ident::new(&naming::to_snake_case(name), proc_macro2::Span::call_site())
}

fn rust_type_to_ffi_param_type(ty: &syn::Type) -> proc_macro2::TokenStream {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    match type_str.as_str() {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool"
        | "usize" | "isize" => quote!(#ty),
        "&str" => quote!(*const std::os::raw::c_char),
        "String" => quote!(*const std::os::raw::c_char),
        _ => quote!(#ty),
    }
}
