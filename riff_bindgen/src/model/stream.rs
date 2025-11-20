use serde::{Deserialize, Serialize};

use super::types::{Deprecation, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StreamMode {
    #[default]
    Async,
    Batch,
    Callback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMethod {
    pub name: String,
    pub item_type: Type,
    pub mode: StreamMode,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl StreamMethod {
    pub fn new(name: impl Into<String>, item_type: Type) -> Self {
        Self {
            name: name.into(),
            item_type,
            mode: StreamMode::default(),
            doc: None,
            deprecated: None,
        }
    }

    pub fn with_mode(mut self, mode: StreamMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn ffi_subscribe(&self, class_prefix: &str) -> String {
        format!("{}_{}", class_prefix, self.name.to_lowercase())
    }

    pub fn ffi_pop_batch(&self, class_prefix: &str) -> String {
        format!("{}_{}_pop_batch", class_prefix, self.name.to_lowercase())
    }

    pub fn ffi_wait(&self, class_prefix: &str) -> String {
        format!("{}_{}_wait", class_prefix, self.name.to_lowercase())
    }

    pub fn ffi_poll(&self, class_prefix: &str) -> String {
        format!("{}_{}_poll", class_prefix, self.name.to_lowercase())
    }

    pub fn ffi_unsubscribe(&self, class_prefix: &str) -> String {
        format!("{}_{}_unsubscribe", class_prefix, self.name.to_lowercase())
    }

    pub fn ffi_free(&self, class_prefix: &str) -> String {
        format!("{}_{}_free", class_prefix, self.name.to_lowercase())
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }
}
