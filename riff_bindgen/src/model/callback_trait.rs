use serde::{Deserialize, Serialize};

use super::types::{Deprecation, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackTrait {
    pub name: String,
    pub methods: Vec<TraitMethod>,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl CallbackTrait {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            methods: Vec::new(),
            doc: None,
            deprecated: None,
        }
    }

    pub fn with_method(mut self, method: TraitMethod) -> Self {
        self.methods.push(method);
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn ffi_vtable_name(&self) -> String {
        format!("{}VTable", self.name)
    }

    pub fn ffi_foreign_name(&self) -> String {
        format!("Foreign{}", self.name)
    }

    pub fn ffi_register_fn(&self, prefix: &str) -> String {
        format!("{}_register_{}_vtable", prefix, to_snake_case(&self.name))
    }

    pub fn ffi_create_fn(&self, prefix: &str) -> String {
        format!("{}_create_{}", prefix, to_snake_case(&self.name))
    }

    pub fn sync_methods(&self) -> impl Iterator<Item = &TraitMethod> {
        self.methods.iter().filter(|m| !m.is_async)
    }

    pub fn async_methods(&self) -> impl Iterator<Item = &TraitMethod> {
        self.methods.iter().filter(|m| m.is_async)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitMethod {
    pub name: String,
    pub inputs: Vec<TraitMethodParam>,
    pub output: Option<Type>,
    pub error: Option<Type>,
    pub is_async: bool,
    pub doc: Option<String>,
}

impl TraitMethod {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            output: None,
            error: None,
            is_async: false,
            doc: None,
        }
    }

    pub fn with_param(mut self, param: TraitMethodParam) -> Self {
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

    pub fn throws(&self) -> bool {
        self.error.is_some()
    }

    pub fn has_return(&self) -> bool {
        self.output.as_ref().map_or(false, |t| !t.is_void())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitMethodParam {
    pub name: String,
    pub param_type: Type,
}

impl TraitMethodParam {
    pub fn new(name: impl Into<String>, param_type: Type) -> Self {
        Self {
            name: name.into(),
            param_type,
        }
    }
}

fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}
