use serde::{Deserialize, Serialize};

use super::method::Method;
use super::stream::StreamMethod;
use super::types::Deprecation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Class {
    pub name: String,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
    pub constructors: Vec<Constructor>,
    pub methods: Vec<Method>,
    pub streams: Vec<StreamMethod>,
}

impl Class {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            doc: None,
            deprecated: None,
            constructors: Vec::new(),
            methods: Vec::new(),
            streams: Vec::new(),
        }
    }

    pub fn ffi_prefix(&self, module_prefix: &str) -> String {
        format!("{}_{}", module_prefix, self.name.to_lowercase())
    }

    pub fn ffi_new(&self, module_prefix: &str) -> String {
        format!("{}_new", self.ffi_prefix(module_prefix))
    }

    pub fn ffi_free(&self, module_prefix: &str) -> String {
        format!("{}_free", self.ffi_prefix(module_prefix))
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn with_constructor(mut self, constructor: Constructor) -> Self {
        self.constructors.push(constructor);
        self
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.methods.push(method);
        self
    }

    pub fn with_stream(mut self, stream: StreamMethod) -> Self {
        self.streams.push(stream);
        self
    }

    pub fn has_constructors(&self) -> bool {
        !self.constructors.is_empty()
    }

    pub fn default_constructor(&self) -> Option<&Constructor> {
        self.constructors.iter().find(|ctor| ctor.inputs.is_empty())
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constructor {
    pub inputs: Vec<ConstructorParam>,
    pub doc: Option<String>,
}

impl Constructor {
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            doc: None,
        }
    }

    pub fn ffi_name(&self, class_prefix: &str) -> String {
        format!("{}_new", class_prefix)
    }

    pub fn with_param(mut self, param: ConstructorParam) -> Self {
        self.inputs.push(param);
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }
}

impl Default for Constructor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructorParam {
    pub name: String,
    pub param_type: super::types::Type,
}

impl ConstructorParam {
    pub fn new(name: impl Into<String>, param_type: super::types::Type) -> Self {
        Self {
            name: name.into(),
            param_type,
        }
    }
}
