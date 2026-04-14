use boltffi_bindgen::render::python::{PythonEmitter, PythonLowerer};

use crate::cli::{CliError, Result};
use crate::commands::generate::generator::{GenerateRequest, LanguageGenerator, ScanPointerWidth};
use crate::config::Target;

pub struct PythonGenerator;

impl PythonGenerator {
    #[cfg(test)]
    pub fn generate_from_source_directory(
        config: &crate::config::Config,
        output_override: Option<std::path::PathBuf>,
        source_directory: &std::path::Path,
        crate_name: &str,
    ) -> Result<()> {
        let request = GenerateRequest::new(
            config,
            output_override,
            crate::commands::generate::generator::SourceCrate::new(source_directory, crate_name),
        );

        Self::generate(&request)
    }
}

impl LanguageGenerator for PythonGenerator {
    const TARGET: Target = Target::Python;

    fn generate(request: &GenerateRequest<'_>) -> Result<()> {
        if !request.config().is_python_enabled() {
            return Err(CliError::CommandFailed {
                command: "targets.python.enabled = false".to_string(),
                status: None,
            });
        }

        let output_directory = request
            .output_override()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| request.config().python_output());

        request.ensure_output_directory(&output_directory)?;

        let lowered_crate = request.lowered_crate(ScanPointerWidth::Host)?;
        let module_name = request.config().python_module_name();
        let python_module = PythonLowerer::new(
            &lowered_crate.ffi_contract,
            &lowered_crate.abi_contract,
            &module_name,
            request.config().package_version(),
        )
        .lower();
        let python_source = PythonEmitter::emit(&python_module);
        let output_path = output_directory.join(format!("{module_name}.py"));

        request.write_output(&output_path, python_source)
    }
}
