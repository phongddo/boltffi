use proc_macro::TokenStream;
use quote::quote;

use crate::wire_gen;

pub fn data_impl(item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(mut item_struct) = syn::parse::<syn::ItemStruct>(item_clone.clone()) {
        let has_repr = item_struct.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            item_struct.attrs.insert(0, syn::parse_quote!(#[repr(C)]));
        }
        
        let wire_impls = wire_gen::generate_wire_impls(&item_struct);
        
        return TokenStream::from(quote! {
            #item_struct
            #wire_impls
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
        return TokenStream::from(quote!(#item_enum));
    }

    syn::Error::new_spanned(
        proc_macro2::TokenStream::from(item),
        "data can only be applied to struct or enum",
    )
    .to_compile_error()
    .into()
}

pub fn derive_data_impl(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
