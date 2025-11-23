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
    BoxedTrait(syn::Ident),
    VecParam(syn::Type),
}

pub fn extract_fn_arg_types(ty: &Type) -> Option<Vec<syn::Type>> {
    if let Type::BareFn(bare_fn) = ty {
        let args: Vec<syn::Type> = bare_fn.inputs.iter().map(|arg| arg.ty.clone()).collect();
        return Some(args);
    }

    if let Type::ImplTrait(impl_trait) = ty {
        for bound in &impl_trait.bounds {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                let path = &trait_bound.path;
                if let Some(segment) = path.segments.last() {
                    let ident = segment.ident.to_string();
                    if (ident == "Fn" || ident == "FnMut" || ident == "FnOnce")
                        && let syn::PathArguments::Parenthesized(args) = &segment.arguments
                    {
                        let arg_types: Vec<syn::Type> = args.inputs.iter().cloned().collect();
                        return Some(arg_types);
                    }
                }
            }
        }
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

pub fn extract_boxed_dyn_trait(ty: &Type) -> Option<syn::Ident> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Box"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) = args.args.first()
        && let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
        && let Some(seg) = trait_bound.path.segments.last()
    {
        return Some(seg.ident.clone());
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

    if let Some(trait_name) = extract_boxed_dyn_trait(ty) {
        return ParamTransform::BoxedTrait(trait_name);
    }

    if type_str.starts_with("*const") || type_str.starts_with("*mut") {
        return ParamTransform::PassThrough;
    }

    if type_str.contains("extern") && type_str.contains("fn(") {
        return ParamTransform::PassThrough;
    }

    if let Some(inner_ty) = extract_vec_param_inner(ty) {
        return ParamTransform::VecParam(inner_ty);
    }

    if type_str == "&str" || (type_str.starts_with("&'") && type_str.ends_with("str")) {
        ParamTransform::StrRef
    } else if type_str == "String" || type_str == "std::string::String" {
        ParamTransform::OwnedString
    } else {
        ParamTransform::PassThrough
    }
}
