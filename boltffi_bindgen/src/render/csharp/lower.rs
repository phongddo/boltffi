use boltffi_ffi_rules::naming;

use crate::ir::definitions::{FunctionDef, ParamDef, ParamPassing, ReturnDef};
use crate::ir::types::TypeExpr;
use crate::ir::{AbiContract, FfiContract};

use super::mappings;
use super::plan::{CSharpFunction, CSharpModule, CSharpParam, CSharpType};
use super::{CSharpOptions, NamingConvention};

/// Transforms the language-agnostic [`FfiContract`] and [`AbiContract`] into
/// a [`CSharpModule`] containing everything the C# templates need to render.
pub struct CSharpLowerer<'a> {
    ffi: &'a FfiContract,
    #[allow(dead_code)]
    abi: &'a AbiContract,
    options: &'a CSharpOptions,
}

impl<'a> CSharpLowerer<'a> {
    pub fn new(ffi: &'a FfiContract, abi: &'a AbiContract, options: &'a CSharpOptions) -> Self {
        Self { ffi, abi, options }
    }

    /// Walk the contracts and produce a C# module plan.
    pub fn lower(&self) -> CSharpModule {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| naming::library_name(&self.ffi.package.name));

        let class_name = NamingConvention::class_name(&self.ffi.package.name);
        let namespace = NamingConvention::namespace(&self.ffi.package.name);
        let prefix = naming::ffi_prefix().to_string();

        let functions: Vec<CSharpFunction> = self
            .ffi
            .functions
            .iter()
            .filter_map(Self::lower_function)
            .collect();

        CSharpModule {
            namespace,
            class_name,
            lib_name,
            prefix,
            functions,
        }
    }

    /// Converts a Rust FFI function definition into its C# representation,
    /// mapping Rust types to C# types and snake_case names to PascalCase.
    ///
    /// Returns `None` for functions whose signatures include types not yet
    /// supported by the C# backend. Once the backend is fully implemented
    /// and the experimental flag is removed, this will succeed for all
    /// functions and the `Option` return will be replaced with a direct
    /// return.
    fn lower_function(function: &FunctionDef) -> Option<CSharpFunction> {
        if function.is_async() {
            return None;
        }

        let params: Vec<CSharpParam> = function
            .params
            .iter()
            .map(Self::lower_param)
            .collect::<Option<Vec<_>>>()?;

        let return_type = Self::lower_return(&function.returns)?;

        Some(CSharpFunction {
            name: NamingConvention::method_name(function.id.as_str()),
            ffi_name: naming::function_ffi_name(function.id.as_str()).into_string(),
            params,
            return_type,
        })
    }

    fn lower_param(param: &ParamDef) -> Option<CSharpParam> {
        if param.passing != ParamPassing::Value {
            return None;
        }

        let csharp_type = Self::lower_type(&param.type_expr)?;

        Some(CSharpParam {
            name: NamingConvention::field_name(param.name.as_str()),
            csharp_type,
        })
    }

    fn lower_return(return_def: &ReturnDef) -> Option<CSharpType> {
        match return_def {
            ReturnDef::Void => Some(CSharpType::Void),
            ReturnDef::Value(type_expr) => Self::lower_type(type_expr),
            ReturnDef::Result { .. } => None,
        }
    }

    fn lower_type(type_expr: &TypeExpr) -> Option<CSharpType> {
        match type_expr {
            TypeExpr::Primitive(primitive) => Some(mappings::csharp_type(*primitive)),
            _ => None,
        }
    }
}
