use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, ItemStruct, Type};

pub fn generate_wire_impls(item_struct: &ItemStruct) -> TokenStream {
    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();
    
    let fields = match &item_struct.fields {
        Fields::Named(named) => &named.named,
        _ => return quote! {},
    };
    
    if fields.is_empty() {
        return generate_empty_struct_impls(struct_name);
    }
    
    let field_names: Vec<_> = fields
        .iter()
        .filter_map(|f| f.ident.as_ref())
        .collect();
    
    let field_types: Vec<_> = fields
        .iter()
        .map(|f| &f.ty)
        .collect();
    
    let field_count = field_names.len();
    let field_indices: Vec<_> = (0..field_count).collect();
    
    let wire_size_impl = generate_wire_size_impl(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &field_names,
        &field_types,
        field_count,
    );
    
    let wire_encode_impl = generate_wire_encode_impl(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &field_names,
        &field_types,
        field_count,
        &field_indices,
    );
    
    let wire_decode_impl = generate_wire_decode_impl(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &field_names,
        &field_types,
        field_count,
        &field_indices,
    );
    
    quote! {
        #wire_size_impl
        #wire_encode_impl
        #wire_decode_impl
    }
}

fn generate_empty_struct_impls(struct_name: &syn::Ident) -> TokenStream {
    quote! {
        impl riff_core::wire::WireSize for #struct_name {
            fn is_fixed_size() -> bool { true }
            fn fixed_size() -> Option<usize> { Some(2) }
            fn wire_size(&self) -> usize { 2 }
        }
        
        impl riff_core::wire::WireEncode for #struct_name {
            fn encode_to(&self, buf: &mut [u8]) -> usize {
                buf[0..2].copy_from_slice(&0u16.to_le_bytes());
                2
            }
        }
        
        impl riff_core::wire::WireDecode for #struct_name {
            fn decode_from(buf: &[u8]) -> riff_core::wire::DecodeResult<Self> {
                if buf.len() < 2 {
                    return Err(riff_core::wire::DecodeError::BufferTooSmall);
                }
                Ok((Self {}, 2))
            }
        }
    }
}

fn generate_wire_size_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_names: &[&syn::Ident],
    field_types: &[&Type],
    field_count: usize,
) -> TokenStream {
    let all_fixed_check = field_types.iter().map(|ty| {
        quote! { <#ty as riff_core::wire::WireSize>::is_fixed_size() }
    });
    
    let fixed_size_sum = field_types.iter().map(|ty| {
        quote! { <#ty as riff_core::wire::WireSize>::fixed_size().unwrap() }
    });
    
    let field_wire_sizes = field_names.iter().map(|name| {
        quote! { riff_core::wire::WireSize::wire_size(&self.#name) }
    });
    
    let field_count_u16 = field_count as u16;
    
    quote! {
        impl #impl_generics riff_core::wire::WireSize for #struct_name #ty_generics #where_clause {
            fn is_fixed_size() -> bool {
                #(#all_fixed_check)&&*
            }
            
            fn fixed_size() -> Option<usize> {
                if Self::is_fixed_size() {
                    Some(#(#fixed_size_sum)+*)
                } else {
                    None
                }
            }
            
            fn wire_size(&self) -> usize {
                if Self::is_fixed_size() {
                    Self::fixed_size().unwrap()
                } else {
                    let header_size = riff_core::wire::FIELD_COUNT_SIZE 
                        + (#field_count_u16 as usize * riff_core::wire::OFFSET_SIZE);
                    let fields_size = #(#field_wire_sizes)+*;
                    header_size + fields_size
                }
            }
        }
    }
}

fn generate_wire_encode_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_names: &[&syn::Ident],
    _field_types: &[&Type],
    field_count: usize,
    field_indices: &[usize],
) -> TokenStream {
    let field_count_u16 = field_count as u16;
    
    let fixed_encode_fields = field_names.iter().map(|name| {
        quote! {
            written += riff_core::wire::WireEncode::encode_to(&self.#name, &mut buf[written..]);
        }
    });
    
    let variable_encode_setup = quote! {
        buf[0..2].copy_from_slice(&(#field_count_u16 as u16).to_le_bytes());
        let offsets_start = riff_core::wire::FIELD_COUNT_SIZE;
        let offsets_size = #field_count * riff_core::wire::OFFSET_SIZE;
        let mut data_position = offsets_start + offsets_size;
    };
    
    let variable_encode_fields = field_names.iter().zip(field_indices.iter()).map(|(name, idx)| {
        let offset_position = quote! { offsets_start + #idx * riff_core::wire::OFFSET_SIZE };
        quote! {
            let field_offset = data_position as u32;
            buf[#offset_position..#offset_position + 4].copy_from_slice(&field_offset.to_le_bytes());
            let field_size = riff_core::wire::WireEncode::encode_to(&self.#name, &mut buf[data_position..]);
            data_position += field_size;
        }
    });
    
    quote! {
        impl #impl_generics riff_core::wire::WireEncode for #struct_name #ty_generics #where_clause {
            fn encode_to(&self, buf: &mut [u8]) -> usize {
                if Self::is_fixed_size() {
                    let mut written = 0usize;
                    #(#fixed_encode_fields)*
                    written
                } else {
                    #variable_encode_setup
                    #(#variable_encode_fields)*
                    data_position
                }
            }
        }
    }
}

fn generate_wire_decode_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_names: &[&syn::Ident],
    field_types: &[&Type],
    field_count: usize,
    field_indices: &[usize],
) -> TokenStream {
    
    let fixed_decode_fields = field_names.iter().zip(field_types.iter()).map(|(name, ty)| {
        quote! {
            let (#name, size) = <#ty as riff_core::wire::WireDecode>::decode_from(&buf[position..])?;
            position += size;
        }
    });
    
    let variable_decode_fields = field_names.iter().zip(field_types.iter()).zip(field_indices.iter())
        .map(|((name, ty), idx)| {
            let offset_position = quote! { offsets_start + #idx * riff_core::wire::OFFSET_SIZE };
            quote! {
                let field_offset = u32::from_le_bytes(
                    buf[#offset_position..#offset_position + 4].try_into().unwrap()
                ) as usize;
                let (#name, field_size) = <#ty as riff_core::wire::WireDecode>::decode_from(&buf[field_offset..])?;
                if field_offset + field_size > max_position {
                    max_position = field_offset + field_size;
                }
            }
        });
    
    let field_names_for_struct: Vec<_> = field_names.iter().map(|n| quote! { #n }).collect();
    
    quote! {
        impl #impl_generics riff_core::wire::WireDecode for #struct_name #ty_generics #where_clause {
            fn decode_from(buf: &[u8]) -> riff_core::wire::DecodeResult<Self> {
                if <Self as riff_core::wire::WireSize>::is_fixed_size() {
                    let mut position = 0usize;
                    #(#fixed_decode_fields)*
                    Ok((Self { #(#field_names_for_struct),* }, position))
                } else {
                    if buf.len() < riff_core::wire::FIELD_COUNT_SIZE {
                        return Err(riff_core::wire::DecodeError::BufferTooSmall);
                    }
                    let field_count = u16::from_le_bytes(buf[0..2].try_into().unwrap()) as usize;
                    if field_count != #field_count {
                        return Err(riff_core::wire::DecodeError::BufferTooSmall);
                    }
                    let offsets_start = riff_core::wire::FIELD_COUNT_SIZE;
                    let offsets_size = field_count * riff_core::wire::OFFSET_SIZE;
                    let mut max_position = offsets_start + offsets_size;
                    
                    #(#variable_decode_fields)*
                    
                    Ok((Self { #(#field_names_for_struct),* }, max_position))
                }
            }
        }
    }
}
