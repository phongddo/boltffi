use serde::{Deserialize, Serialize};

use super::types::{Deprecation, Receiver, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    pub name: String,
    pub receiver: Receiver,
    pub inputs: Vec<Parameter>,
    pub output: Option<Type>,
    pub error: Option<Type>,
    pub is_async: bool,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl Method {
    pub fn new(name: impl Into<String>, receiver: Receiver) -> Self {
        Self {
            name: name.into(),
            receiver,
            inputs: Vec::new(),
            output: None,
            error: None,
            is_async: false,
            doc: None,
            deprecated: None,
        }
    }

    pub fn ffi_name(&self, class_prefix: &str) -> String {
        format!("{}_{}", class_prefix, self.name.to_lowercase())
    }

    pub fn ffi_poll(&self, class_prefix: &str) -> String {
        format!("{}_poll", self.ffi_name(class_prefix))
    }

    pub fn ffi_complete(&self, class_prefix: &str) -> String {
        format!("{}_complete", self.ffi_name(class_prefix))
    }

    pub fn ffi_cancel(&self, class_prefix: &str) -> String {
        format!("{}_cancel", self.ffi_name(class_prefix))
    }

    pub fn ffi_free(&self, class_prefix: &str) -> String {
        format!("{}_free", self.ffi_name(class_prefix))
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

    pub fn is_static(&self) -> bool {
        self.receiver.is_static()
    }

    pub fn is_mutating(&self) -> bool {
        self.receiver.is_mutable()
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    pub fn has_return_value(&self) -> bool {
        self.output
            .as_ref()
            .map_or(false, |output| !output.is_void())
    }

    pub fn has_callbacks(&self) -> bool {
        self.inputs
            .iter()
            .any(|p| matches!(p.param_type, Type::Callback(_)))
    }

    pub fn callback_params(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs
            .iter()
            .filter(|p| matches!(p.param_type, Type::Callback(_)))
    }

    pub fn non_callback_params(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs
            .iter()
            .filter(|p| !matches!(p.param_type, Type::Callback(_)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: Type,
}

impl Parameter {
    pub fn new(name: impl Into<String>, param_type: Type) -> Self {
        Self {
            name: name.into(),
            param_type,
        }
    }
}
