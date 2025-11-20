use serde::{Deserialize, Serialize};

use super::method::Parameter;
use super::types::{Deprecation, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub inputs: Vec<Parameter>,
    pub output: Option<Type>,
    pub error: Option<Type>,
    pub is_async: bool,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl Function {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            output: None,
            error: None,
            is_async: false,
            doc: None,
            deprecated: None,
        }
    }

    pub fn ffi_name(&self, module_prefix: &str) -> String {
        format!("{}_{}", module_prefix, self.name.to_lowercase())
    }

    pub fn ffi_poll(&self, module_prefix: &str) -> String {
        format!("{}_poll", self.ffi_name(module_prefix))
    }

    pub fn ffi_complete(&self, module_prefix: &str) -> String {
        format!("{}_complete", self.ffi_name(module_prefix))
    }

    pub fn ffi_cancel(&self, module_prefix: &str) -> String {
        format!("{}_cancel", self.ffi_name(module_prefix))
    }

    pub fn ffi_free(&self, module_prefix: &str) -> String {
        format!("{}_free", self.ffi_name(module_prefix))
    }

    pub fn with_param(mut self, param: Parameter) -> Self {
        self.inputs.push(param);
        self
    }

    pub fn with_output(mut self, output: Type) -> Self {
        self.output = Some(output);
        self
    }

    pub fn with_error(mut self, error: Type) -> Self {
        self.error = Some(error);
        self
    }

    pub fn make_async(mut self) -> Self {
        self.is_async = true;
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn throws(&self) -> bool {
        self.error.is_some()
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    pub fn has_return_value(&self) -> bool {
        self.output
            .as_ref()
            .map_or(false, |output| !output.is_void())
    }
}
