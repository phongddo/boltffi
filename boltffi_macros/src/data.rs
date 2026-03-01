use boltffi_ffi_rules::classification::{self, FieldPrimitive, PassableCategory};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::Fields;

use crate::custom_types;
use crate::data_types::{extract_integer_repr, has_repr_c};
use crate::wire_gen;

fn is_c_style_enum(item_enum: &syn::ItemEnum) -> bool {
    item_enum.variants.iter().all(|v| v.fields.is_empty())
}

fn has_integer_repr(attrs: &[syn::Attribute]) -> bool {
    extract_integer_repr(attrs).is_some()
}

fn type_to_field_primitive(ty: &syn::Type) -> Option<FieldPrimitive> {
    match ty {
        syn::Type::Path(path) => path
            .path
            .get_ident()
            .and_then(|ident| FieldPrimitive::from_type_name(&ident.to_string())),
        _ => None,
    }
}

fn generate_passable_for_scalar_enum(
    enum_name: &syn::Ident,
    repr_type: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> proc_macro2::TokenStream {
    let match_arms: Vec<proc_macro2::TokenStream> = variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            quote! { value if value == (#enum_name::#variant_name as #repr_type) => #enum_name::#variant_name }
        })
        .collect();

    quote! {
        unsafe impl ::boltffi::__private::Passable for #enum_name {
            type In = #repr_type;
            type Out = #repr_type;

            fn pack(self) -> #repr_type {
                self as #repr_type
            }

            unsafe fn unpack(input: #repr_type) -> Self {
                match input {
                    #(#match_arms,)*
                    _ => ::core::panic!("invalid enum discriminant"),
                }
            }
        }

        impl ::boltffi::__private::VecTransport<#enum_name> for ::boltffi::__private::Seal {
            fn pack(vec: Vec<#enum_name>) -> ::boltffi::__private::FfiBuf {
                ::boltffi::__private::FfiBuf::from_vec(vec)
            }
            unsafe fn unpack(ptr: *const u8, byte_len: usize) -> Vec<#enum_name> {
                let count = byte_len / ::core::mem::size_of::<#enum_name>();
                unsafe { ::core::slice::from_raw_parts(ptr as *const #enum_name, count) }.to_vec()
            }
        }
    }
}

fn generate_passable_for_wire_encoded(name: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        unsafe impl ::boltffi::__private::WirePassable for #name {}

        impl ::boltffi::__private::VecTransport<#name> for ::boltffi::__private::Seal {
            fn pack(vec: Vec<#name>) -> ::boltffi::__private::FfiBuf {
                ::boltffi::__private::FfiBuf::wire_encode(&vec)
            }
            unsafe fn unpack(ptr: *const u8, byte_len: usize) -> Vec<#name> {
                let bytes = unsafe { ::core::slice::from_raw_parts(ptr, byte_len) };
                ::boltffi::__private::wire::decode(bytes).expect("VecTransport::unpack: wire decode failed")
            }
        }
    }
}

fn generate_passable_for_blittable_struct(struct_name: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        unsafe impl ::boltffi::__private::Passable for #struct_name {
            type In = #struct_name;
            type Out = #struct_name;

            fn pack(self) -> #struct_name {
                self
            }

            unsafe fn unpack(input: #struct_name) -> Self {
                input
            }
        }

        impl ::boltffi::__private::VecTransport<#struct_name> for ::boltffi::__private::Seal {
            fn pack(vec: Vec<#struct_name>) -> ::boltffi::__private::FfiBuf {
                ::boltffi::__private::FfiBuf::from_vec(vec)
            }
            unsafe fn unpack(ptr: *const u8, byte_len: usize) -> Vec<#struct_name> {
                let count = byte_len / ::core::mem::size_of::<#struct_name>();
                unsafe { ::core::slice::from_raw_parts(ptr as *const #struct_name, count) }.to_vec()
            }
        }
    }
}

pub fn data_impl(item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(mut item_struct) = syn::parse::<syn::ItemStruct>(item_clone.clone()) {
        let has_repr = item_struct.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            item_struct.attrs.insert(0, syn::parse_quote!(#[repr(C)]));
        }

        strip_boltffi_field_attrs(&mut item_struct.fields);

        let struct_name = &item_struct.ident;
        let free_fn_name = format_ident!("boltffi_free_buf_{}", struct_name);

        let custom_types = match custom_types::registry_for_current_crate() {
            Ok(registry) => registry,
            Err(error) => return error.to_compile_error().into(),
        };
        let wire_impls = wire_gen::generate_wire_impls(&item_struct, &custom_types);

        let struct_has_repr_c = has_repr_c(&item_struct.attrs);
        let field_primitives: Vec<FieldPrimitive> = match &item_struct.fields {
            Fields::Named(named) => named
                .named
                .iter()
                .filter_map(|f| type_to_field_primitive(&f.ty))
                .collect(),
            Fields::Unnamed(unnamed) => unnamed
                .unnamed
                .iter()
                .filter_map(|f| type_to_field_primitive(&f.ty))
                .collect(),
            Fields::Unit => vec![],
        };
        let total_fields = match &item_struct.fields {
            Fields::Named(named) => named.named.len(),
            Fields::Unnamed(unnamed) => unnamed.unnamed.len(),
            Fields::Unit => 0,
        };
        let all_primitive = field_primitives.len() == total_fields;
        let classify_fields: Vec<FieldPrimitive> = if all_primitive {
            field_primitives
        } else {
            vec![]
        };
        let passable_impl =
            match classification::classify_struct(struct_has_repr_c, &classify_fields) {
                PassableCategory::Blittable => generate_passable_for_blittable_struct(struct_name),
                _ => generate_passable_for_wire_encoded(struct_name),
            };

        return TokenStream::from(quote! {
            #item_struct
            #wire_impls
            #passable_impl

            #[cfg(not(test))]
            #[unsafe(no_mangle)]
            pub extern "C" fn #free_fn_name(buf: ::boltffi::__private::FfiBuf) {
                drop(buf);
            }
        });
    }

    if let Ok(mut item_enum) = syn::parse::<syn::ItemEnum>(item_clone) {
        let has_repr = item_enum.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            let has_data = item_enum.variants.iter().any(|v| !v.fields.is_empty());
            if has_data {
                item_enum
                    .attrs
                    .insert(0, syn::parse_quote!(#[repr(C, i32)]));
            } else {
                item_enum.attrs.insert(0, syn::parse_quote!(#[repr(i32)]));
            }
        }

        let custom_types = match custom_types::registry_for_current_crate() {
            Ok(registry) => registry,
            Err(error) => return error.to_compile_error().into(),
        };
        let wire_impls = wire_gen::generate_enum_wire_impls(&item_enum, &custom_types);

        let enum_name = &item_enum.ident;
        let passable_impl = match classification::classify_enum(
            is_c_style_enum(&item_enum),
            has_integer_repr(&item_enum.attrs),
        ) {
            PassableCategory::Scalar => {
                let repr_type = extract_integer_repr(&item_enum.attrs)
                    .unwrap_or_else(|| syn::Ident::new("i32", enum_name.span()));
                generate_passable_for_scalar_enum(enum_name, &repr_type, &item_enum.variants)
            }
            _ => generate_passable_for_wire_encoded(enum_name),
        };

        return TokenStream::from(quote! {
            #item_enum
            #wire_impls
            #passable_impl
        });
    }

    syn::Error::new_spanned(
        proc_macro2::TokenStream::from(item),
        "data can only be applied to struct or enum",
    )
    .to_compile_error()
    .into()
}

fn is_boltffi_field_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    path.segments.len() == 2
        && path.segments[0].ident == "boltffi"
        && path.segments[1].ident == "default"
}

fn strip_boltffi_field_attrs(fields: &mut syn::Fields) {
    fields.iter_mut().for_each(|field| {
        field.attrs.retain(|attr| !is_boltffi_field_attr(attr));
    });
}

pub fn derive_data_impl(_input: TokenStream) -> TokenStream {
    let derive_input = match syn::parse::<syn::DeriveInput>(_input) {
        Ok(derive_input) => derive_input,
        Err(error) => return error.to_compile_error().into(),
    };

    syn::Error::new_spanned(
        derive_input.ident,
        "#[derive(Data)] is not supported; use #[data] or #[error] instead",
    )
    .to_compile_error()
    .into()
}
