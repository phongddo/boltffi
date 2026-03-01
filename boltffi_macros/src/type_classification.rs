use boltffi_ffi_rules::primitive::Primitive;
use syn::Type;

use crate::custom_types::{self, CustomTypeRegistry, contains_custom_types};
use crate::data_types::{self, DataTypeCategory, DataTypeRegistry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedTypeTransport {
    Passable,
    WireEncoded,
}

pub fn classify_named_type_transport(
    ty: &Type,
    custom_types: &CustomTypeRegistry,
    data_types: &DataTypeRegistry,
) -> NamedTypeTransport {
    if contains_custom_types(ty, custom_types) {
        return NamedTypeTransport::WireEncoded;
    }

    if data_types.category_for(ty).is_some() {
        return NamedTypeTransport::Passable;
    }

    NamedTypeTransport::WireEncoded
}

pub fn classify_named_type_transport_for_call_site(ty: &Type) -> NamedTypeTransport {
    let custom_types = custom_types::registry_for_current_crate().ok();
    let data_types = data_types::registry_for_current_crate().ok();

    match (custom_types, data_types) {
        (Some(custom_types), Some(data_types)) => {
            classify_named_type_transport(ty, &custom_types, &data_types)
        }
        (Some(_), None) => NamedTypeTransport::WireEncoded,
        (None, Some(data_types)) => data_types
            .category_for(ty)
            .map(|_| NamedTypeTransport::Passable)
            .unwrap_or(NamedTypeTransport::WireEncoded),
        (None, None) => NamedTypeTransport::WireEncoded,
    }
}

pub fn supports_direct_vec_transport(
    ty: &Type,
    custom_types: &CustomTypeRegistry,
    data_types: &DataTypeRegistry,
) -> bool {
    if is_primitive_type(ty) {
        return true;
    }

    if is_string_like_type(ty) || contains_custom_types(ty, custom_types) {
        return false;
    }

    data_types
        .category_for(ty)
        .is_some_and(DataTypeCategory::supports_direct_vec)
}

fn is_primitive_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| ident.to_string().parse::<Primitive>().is_ok()),
        _ => false,
    }
}

fn is_string_like_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "String"),
        Type::Reference(reference) => match reference.elem.as_ref() {
            Type::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "str"),
            _ => false,
        },
        _ => false,
    }
}
