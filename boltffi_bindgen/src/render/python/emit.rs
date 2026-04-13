use askama::Template as _;

use crate::render::python::PythonModule;
use crate::render::python::templates::ModuleTemplate;

pub struct PythonEmitter;

impl PythonEmitter {
    pub fn emit(module: &PythonModule) -> String {
        ModuleTemplate { module }.render().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::PythonEmitter;
    use crate::render::python::{PythonExportCounts, PythonModule};

    #[test]
    fn emits_importable_python_module_scaffold() {
        let module = PythonModule {
            module_name: "demo_lib".to_string(),
            package_name: "demo-lib".to_string(),
            package_version: Some("0.1.0".to_string()),
            exported_api: PythonExportCounts {
                functions: 2,
                records: 1,
                enumerations: 1,
                classes: 1,
                callbacks: 1,
            },
        };

        let rendered = PythonEmitter::emit(&module);

        assert!(rendered.contains("MODULE_NAME = \"demo_lib\""));
        assert!(rendered.contains("PACKAGE_NAME = \"demo-lib\""));
        assert!(rendered.contains("\"callbacks\": 1"));
        assert!(rendered.contains("__all__ = ["));
    }
}
