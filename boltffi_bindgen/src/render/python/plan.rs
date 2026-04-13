#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonExportCounts {
    pub functions: usize,
    pub records: usize,
    pub enumerations: usize,
    pub classes: usize,
    pub callbacks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonModule {
    pub module_name: String,
    pub package_name: String,
    pub package_version: Option<String>,
    pub exported_api: PythonExportCounts,
}

impl PythonModule {
    pub fn module_name_literal(&self) -> String {
        format!("{:?}", self.module_name)
    }

    pub fn package_name_literal(&self) -> String {
        format!("{:?}", self.package_name)
    }

    pub fn package_version_literal(&self) -> String {
        self.package_version
            .as_ref()
            .map(|version| format!("{version:?}"))
            .unwrap_or_else(|| "None".to_string())
    }
}
