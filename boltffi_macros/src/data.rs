use boltffi_ffi_rules::classification::{self, FieldPrimitive, PassableCategory};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::Fields;

use crate::custom_types;
use crate::wire_gen;

fn is_c_style_enum(item_enum: &syn::ItemEnum) -> bool {
    item_enum.variants.iter().all(|v| v.fields.is_empty())
}

fn has_integer_repr(attrs: &[syn::Attribute]) -> bool {
    extract_integer_repr(attrs).is_some()
}

fn extract_integer_repr(attrs: &[syn::Attribute]) -> Option<syn::Ident> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("repr"))
        .find_map(|attr| {
            attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
            )
            .ok()
            .and_then(|idents| {
                idents.into_iter().find(|ident| {
                    matches!(
                        ident.to_string().as_str(),
                        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize"
                    )
                })
            })
        })
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
) -> proc_macro2::TokenStream {
    quote! {
        unsafe impl ::boltffi::__private::Passable for #enum_name {
            type In = #repr_type;
            type Out = #repr_type;

            fn pack(self) -> #repr_type {
                self as #repr_type
            }

            unsafe fn unpack(input: #repr_type) -> Self {
                unsafe { ::core::mem::transmute(input) }
            }
        }
    }
}

fn generate_passable_for_wire_encoded(name: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        unsafe impl ::boltffi::__private::WirePassable for #name {}
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

        let has_repr_c = item_struct.attrs.iter().any(|a| {
            a.path().is_ident("repr")
                && a.parse_args::<syn::Ident>()
                    .is_ok_and(|ident| ident == "C")
        });
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
            match classification::classify_struct(has_repr_c, &classify_fields) {
                PassableCategory::Blittable => {
                    generate_passable_for_blittable_struct(struct_name)
                }
                _ => generate_passable_for_wire_encoded(struct_name),
            };

        return TokenStream::from(quote! {
            #item_struct
            #wire_impls
            #passable_impl

            #[cfg(not(test))]
            #[unsafe(no_mangle)]
            pub extern "C" fn #free_fn_name(buf: ::boltffi::__private::FfiBuf<#struct_name>) {
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
                generate_passable_for_scalar_enum(enum_name, &repr_type)
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
    TokenStream::new()
}
