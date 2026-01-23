use quote::quote;
use riff_ffi_rules::naming;
use syn::Type;

pub fn ptr_ident(base: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("{}{}", base, naming::param_ptr_suffix()),
        base.span(),
    )
}

pub fn len_ident(base: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("{}{}", base, naming::param_len_suffix()),
        base.span(),
    )
}

pub enum ParamTransform {
    PassThrough,
    StrRef,
    OwnedString,
    Callback(Vec<syn::Type>),
    SliceRef(syn::Type),
    SliceMut(syn::Type),
    BoxedDynTrait(syn::Path),
    ArcDynTrait(syn::Path),
    OptionArcDynTrait(syn::Path),
    VecPrimitive(syn::Type),
    VecWireEncoded(syn::Type),
    OptionWireEncoded(syn::Type),
    RecordWireEncoded(syn::Type),
}

pub fn extract_fn_arg_types(ty: &Type) -> Option<Vec<syn::Type>> {
    if let Type::BareFn(bare_fn) = ty {
        let args: Vec<syn::Type> = bare_fn.inputs.iter().map(|arg| arg.ty.clone()).collect();
        return Some(args);
    }

    if let Type::ImplTrait(impl_trait) = ty {
        return impl_trait
            .bounds
            .iter()
            .filter_map(|bound| match bound {
                syn::TypeParamBound::Trait(trait_bound) => Some(&trait_bound.path),
                _ => None,
            })
            .filter_map(|path| path.segments.last())
            .filter_map(|segment| {
                let ident = segment.ident.to_string();
                (ident == "Fn" || ident == "FnMut" || ident == "FnOnce")
                    .then_some(&segment.arguments)
            })
            .filter_map(|arguments| match arguments {
                syn::PathArguments::Parenthesized(args) => Some(args),
                _ => None,
            })
            .map(|args| args.inputs.iter().cloned().collect())
            .next();
    }

    None
}

pub fn extract_slice_inner(ty: &Type) -> Option<(syn::Type, bool)> {
    if let Type::Reference(ref_ty) = ty
        && let Type::Slice(slice_ty) = ref_ty.elem.as_ref()
    {
        let is_mut = ref_ty.mutability.is_some();
        return Some((*slice_ty.elem.clone(), is_mut));
    }
    None
}

fn extract_dyn_trait_in_container(ty: &Type, container: &str) -> Option<syn::Path> {
    if let Type::Path(type_path) = ty
        && type_path.qself.is_none()
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == container
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) = args.args.first()
        && let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
    {
        return Some(trait_bound.path.clone());
    }
    None
}

pub fn extract_vec_param_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn extract_option_param_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn is_primitive_vec_inner(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool"
    )
}

pub fn classify_param_transform(ty: &Type) -> ParamTransform {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    if let Some(arg_types) = extract_fn_arg_types(ty) {
        return ParamTransform::Callback(arg_types);
    }

    if let Some((inner_ty, is_mut)) = extract_slice_inner(ty) {
        return if is_mut {
            ParamTransform::SliceMut(inner_ty)
        } else {
            ParamTransform::SliceRef(inner_ty)
        };
    }

    if let Some(trait_path) = extract_dyn_trait_in_container(ty, "Box") {
        return ParamTransform::BoxedDynTrait(trait_path);
    }

    if let Some(trait_path) = extract_dyn_trait_in_container(ty, "Arc") {
        return ParamTransform::ArcDynTrait(trait_path);
    }

    if type_str.starts_with("*const") || type_str.starts_with("*mut") {
        return ParamTransform::PassThrough;
    }

    if type_str.contains("extern") && type_str.contains("fn(") {
        return ParamTransform::PassThrough;
    }

    if let Some(inner_ty) = extract_vec_param_inner(ty) {
        let inner_str = quote!(#inner_ty).to_string().replace(' ', "");
        if is_primitive_vec_inner(&inner_str) {
            return ParamTransform::VecPrimitive(inner_ty);
        } else {
            return ParamTransform::VecWireEncoded(inner_ty);
        }
    }

    if let Some(inner_ty) = extract_option_param_inner(ty) {
        if let Some(trait_path) = extract_dyn_trait_in_container(&inner_ty, "Arc") {
            return ParamTransform::OptionArcDynTrait(trait_path);
        }
        return ParamTransform::OptionWireEncoded(inner_ty);
    }

    if type_str == "&str" || (type_str.starts_with("&'") && type_str.ends_with("str")) {
        ParamTransform::StrRef
    } else if type_str == "String" || type_str == "std::string::String" {
        ParamTransform::OwnedString
    } else if is_record_type(&type_str) {
        ParamTransform::RecordWireEncoded(ty.clone())
    } else {
        ParamTransform::PassThrough
    }
}

fn is_record_type(type_str: &str) -> bool {
    if is_primitive_type(type_str) {
        return false;
    }
    if type_str.starts_with('&') || type_str.starts_with('*') {
        return false;
    }
    if type_str.contains('<') || type_str.contains('>') {
        return false;
    }
    type_str.chars().next().is_some_and(|c| c.is_uppercase())
}

fn is_primitive_type(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "bool"
            | "isize"
            | "usize"
            | "()"
    )
}
