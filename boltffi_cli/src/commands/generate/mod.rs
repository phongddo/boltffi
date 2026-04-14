mod generator;
mod header;
mod languages;

use std::path::{Path, PathBuf};

use generator::{GenerateRequest, run_generator};
use header::HeaderGenerator;
use languages::{
    DartGenerator, JavaGenerator, KotlinGenerator, PythonGenerator, SwiftGenerator,
    TypeScriptGenerator,
};

use crate::cli::Result;
use crate::config::{Config, Target};

pub enum GenerateTarget {
    Swift,
    Kotlin,
    Java,
    Header,
    Typescript,
    Dart,
    Python,
    All,
}

pub struct GenerateOptions {
    pub target: GenerateTarget,
    pub output: Option<PathBuf>,
    pub experimental: bool,
}

pub fn run_generate_with_output(config: &Config, options: GenerateOptions) -> Result<()> {
    let request = GenerateRequest::for_current_crate(config, options.output);

    match options.target {
        GenerateTarget::Swift => run_generator::<SwiftGenerator>(&request, options.experimental),
        GenerateTarget::Kotlin => run_generator::<KotlinGenerator>(&request, options.experimental),
        GenerateTarget::Java => run_generator::<JavaGenerator>(&request, options.experimental),
        GenerateTarget::Header => run_generator::<HeaderGenerator>(&request, options.experimental),
        GenerateTarget::Typescript => {
            run_generator::<TypeScriptGenerator>(&request, options.experimental)
        }
        GenerateTarget::Dart => run_generator::<DartGenerator>(&request, options.experimental),
        GenerateTarget::Python => run_generator::<PythonGenerator>(&request, options.experimental),
        GenerateTarget::All => {
            if config.should_process(Target::Swift, options.experimental) {
                run_generator::<SwiftGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Kotlin, options.experimental) {
                run_generator::<KotlinGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Java, options.experimental) {
                run_generator::<JavaGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Header, options.experimental) {
                run_generator::<HeaderGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::TypeScript, options.experimental) {
                run_generator::<TypeScriptGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Dart, options.experimental) {
                run_generator::<DartGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Python, options.experimental) {
                run_generator::<PythonGenerator>(&request, options.experimental)?;
            }

            Ok(())
        }
    }
}

pub fn run_generate_java_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    JavaGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::languages::PythonGenerator;
    use crate::config::Config;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    fn demo_source_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo")
    }

    #[test]
    fn python_generate_writes_module_file() {
        let output_directory = unique_temp_dir("boltffi-python-generate-test");
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.python]
enabled = true
"#,
        );

        PythonGenerator::generate_from_source_directory(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
        )
        .expect("python generate should succeed");

        let generated_module_path = output_directory.join("demo.py");
        let generated_module = fs::read_to_string(&generated_module_path)
            .expect("generated python module should be readable");

        assert!(generated_module.contains("MODULE_NAME = \"demo\""));
        assert!(generated_module.contains("PACKAGE_NAME = \"demo\""));
        assert!(generated_module.contains("PACKAGE_VERSION = \"0.1.0\""));
        assert!(generated_module.contains("EXPORTED_API = {"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }
}
