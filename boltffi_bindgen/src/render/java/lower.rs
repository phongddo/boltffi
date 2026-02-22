use crate::ir::abi::{AbiCall, AbiContract, CallId};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{FunctionDef, ReturnDef};
use crate::ir::types::TypeExpr;

use super::JavaOptions;
use super::mappings;
use super::names::NamingConvention;
use super::plan::{JavaFunction, JavaModule, JavaParam, JavaParamKind, JavaReturnStrategy};

pub struct JavaLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: JavaOptions,
}

impl<'a> JavaLowerer<'a> {
    pub fn new(
        ffi: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> Self {
        Self {
            ffi,
            abi,
            package_name,
            module_name,
            options,
        }
    }

    pub fn module(&self) -> JavaModule {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| self.module_name.clone())
            .replace('-', "_");

        let prefix = boltffi_ffi_rules::naming::ffi_prefix().to_string();

        let functions: Vec<JavaFunction> = self
            .ffi
            .functions
            .iter()
            .filter(|f| !f.is_async && self.is_supported_function(f))
            .map(|f| self.lower_function(f))
            .collect();

        JavaModule {
            package_name: self.package_name.clone(),
            class_name: NamingConvention::class_name(&self.module_name),
            lib_name,
            java_version: self.options.min_java_version,
            prefix,
            functions,
        }
    }

    fn is_supported_function(&self, func: &FunctionDef) -> bool {
        let params_ok = func
            .params
            .iter()
            .all(|p| self.is_supported_param(&p.type_expr));
        let return_ok = match &func.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_return(ty),
            ReturnDef::Result { .. } => false,
        };
        params_ok && return_ok
    }

    fn is_supported_param(&self, ty: &TypeExpr) -> bool {
        matches!(ty, TypeExpr::Primitive(_) | TypeExpr::String)
    }

    fn is_supported_return(&self, ty: &TypeExpr) -> bool {
        matches!(
            ty,
            TypeExpr::Void | TypeExpr::Primitive(_) | TypeExpr::String
        )
    }

    fn lower_function(&self, func: &FunctionDef) -> JavaFunction {
        let call = self.abi_call_for_function(func);

        let params: Vec<JavaParam> = func
            .params
            .iter()
            .map(|p| self.lower_param(p.name.as_str(), &p.type_expr))
            .collect();

        let strategy = self.return_strategy(&func.returns);

        JavaFunction {
            name: NamingConvention::method_name(func.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            params,
            return_type: self.return_java_type(&func.returns),
            strategy,
        }
    }

    fn lower_param(&self, name: &str, ty: &TypeExpr) -> JavaParam {
        let field_name = NamingConvention::field_name(name);
        let java_type = self.java_type(ty);
        let (native_type, kind) = self.native_param_mapping(ty);
        JavaParam {
            name: field_name,
            java_type,
            native_type,
            kind,
        }
    }

    fn native_param_mapping(&self, ty: &TypeExpr) -> (String, JavaParamKind) {
        match ty {
            TypeExpr::String => ("byte[]".to_string(), JavaParamKind::Utf8Bytes),
            other => (self.java_type(other), JavaParamKind::Direct),
        }
    }

    fn return_java_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "void".to_string(),
            ReturnDef::Value(TypeExpr::Void) => "void".to_string(),
            ReturnDef::Value(ty) => self.java_type(ty),
            ReturnDef::Result { .. } => "void".to_string(),
        }
    }

    fn return_strategy(&self, returns: &ReturnDef) -> JavaReturnStrategy {
        match returns {
            ReturnDef::Void | ReturnDef::Result { .. } => JavaReturnStrategy::Void,
            ReturnDef::Value(ty) => match ty {
                TypeExpr::Void => JavaReturnStrategy::Void,
                TypeExpr::Primitive(_) => JavaReturnStrategy::Direct,
                TypeExpr::String => JavaReturnStrategy::WireDecode {
                    decode_expr: "reader.readString()".to_string(),
                },
                _ => JavaReturnStrategy::Void,
            },
        }
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| matches!(&c.id, CallId::Function(id) if id == &func.id))
            .expect("abi call not found for function")
    }

    fn java_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            _ => "Object".to_string(),
        }
    }
}
