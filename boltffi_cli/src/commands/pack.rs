use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::build::{
    BuildOptions, Builder, CargoBuildProfile, OutputCallback, all_successful, failed_targets,
    resolve_build_profile,
};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::config::{
    Config, SpmDistribution, SpmLayout, Target, WasmNpmTarget, WasmOptimizeLevel,
    WasmOptimizeOnMissing, WasmProfile,
};
use crate::error::{CliError, Result};
use crate::pack::{AndroidPackager, SpmPackageGenerator, XcframeworkBuilder, compute_checksum};
use crate::reporter::{Reporter, Step};
use crate::target::{BuiltLibrary, Platform};

pub enum PackCommand {
    All(PackAllOptions),
    Apple(PackAppleOptions),
    Android(PackAndroidOptions),
    Wasm(PackWasmOptions),
    Java(PackJavaOptions),
}

pub struct PackAllOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub experimental: bool,
    pub cargo_args: Vec<String>,
}

pub struct PackAppleOptions {
    pub release: bool,
    pub version: Option<String>,
    pub regenerate: bool,
    pub no_build: bool,
    pub spm_only: bool,
    pub xcframework_only: bool,
    pub layout: Option<SpmLayout>,
    pub cargo_args: Vec<String>,
}

pub struct PackAndroidOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub cargo_args: Vec<String>,
}

pub struct PackWasmOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub cargo_args: Vec<String>,
}

pub struct PackJavaOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub cargo_args: Vec<String>,
}

pub fn run_pack(config: &Config, command: PackCommand, reporter: &Reporter) -> Result<()> {
    match command {
        PackCommand::All(options) => pack_all(config, options, reporter),
        PackCommand::Apple(options) => pack_apple(config, options, reporter),
        PackCommand::Android(options) => pack_android(config, options, reporter),
        PackCommand::Wasm(options) => pack_wasm(config, options, reporter),
        PackCommand::Java(options) => pack_java(config, options, reporter),
    }
}

fn pack_all(config: &Config, options: PackAllOptions, reporter: &Reporter) -> Result<()> {
    let mut packed_any = false;

    if config.is_apple_enabled() {
        pack_apple(
            config,
            PackAppleOptions {
                release: options.release,
                version: None,
                regenerate: options.regenerate,
                no_build: options.no_build,
                spm_only: false,
                xcframework_only: false,
                layout: None,
                cargo_args: options.cargo_args.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_android_enabled() {
        pack_android(
            config,
            PackAndroidOptions {
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                cargo_args: options.cargo_args.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_wasm_enabled() {
        pack_wasm(
            config,
            PackWasmOptions {
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                cargo_args: options.cargo_args.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.should_process(Target::Java, options.experimental) {
        pack_java(
            config,
            PackJavaOptions {
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                cargo_args: options.cargo_args.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if !packed_any {
        reporter.warning("no targets enabled in boltffi.toml");
    }

    reporter.finish();
    Ok(())
}

fn pack_apple(config: &Config, options: PackAppleOptions, reporter: &Reporter) -> Result<()> {
    if !config.is_apple_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.apple.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🍎", "Packing Apple");

    if !config.apple_include_macos() {
        reporter.warning("macOS excluded (targets.apple.include_macos = false)");
    }

    if options.spm_only && options.xcframework_only {
        return Err(CliError::CommandFailed {
            command: "cannot combine --spm-only and --xcframework-only".to_string(),
            status: None,
        });
    }

    let build_cargo_args = resolve_build_cargo_args(config, &options.cargo_args);
    let build_profile = resolve_build_profile(options.release, &build_cargo_args);

    if !options.no_build {
        let step = reporter.step("Building Apple targets");
        build_apple_targets(config, options.release, &build_cargo_args, &step)?;
        step.finish_success();
    }

    let layout = options.layout.unwrap_or_else(|| config.apple_spm_layout());
    let package_root = config.apple_spm_output();

    if options.regenerate {
        let step = reporter.step("Generating Apple bindings");
        generate_apple_bindings(config, layout, &package_root)?;
        step.finish_success();
    }

    let libraries = discover_built_libraries(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
    )?;
    let apple_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform().is_apple())
        .collect();

    if apple_libraries.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "Apple".to_string(),
        });
    }

    let headers_dir = config.apple_header_output();
    if !headers_dir.exists() {
        return Err(CliError::FileNotFound(headers_dir));
    }

    let should_build_xcframework = !options.spm_only;
    let should_generate_spm = !options.xcframework_only;

    let xcframework_output = if should_build_xcframework {
        let step = reporter.step("Creating xcframework");
        let output = XcframeworkBuilder::new(config, apple_libraries.clone(), headers_dir.clone())
            .build_with_zip()?;
        step.finish_success();
        Some(output)
    } else {
        None
    };

    if should_generate_spm {
        let (checksum, version) = match config.apple_spm_distribution() {
            SpmDistribution::Local => (None, None),
            SpmDistribution::Remote => {
                let checksum = xcframework_output
                    .as_ref()
                    .and_then(|o| o.checksum.clone())
                    .map(Ok)
                    .unwrap_or_else(|| {
                        let step = reporter.step("Computing checksum");
                        let result = existing_xcframework_checksum(config);
                        step.finish_success();
                        result
                    })?;
                let version = options
                    .version
                    .or_else(detect_version)
                    .unwrap_or_else(|| "0.1.0".to_string());
                (Some(checksum), Some(version))
            }
        };

        if config.apple_spm_skip_package_swift() {
            reporter.warning("Skipping Package.swift (skip_package_swift = true)");
        } else {
            let generator = match config.apple_spm_distribution() {
                SpmDistribution::Local => SpmPackageGenerator::new_local(config, layout),
                SpmDistribution::Remote => {
                    let checksum = checksum.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires checksum".to_string(),
                        status: None,
                    })?;
                    let version = version.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires version".to_string(),
                        status: None,
                    })?;
                    SpmPackageGenerator::new_remote(config, checksum, version, layout)
                }
            };

            let step = reporter.step("Generating Package.swift");
            let package_path = generator.generate()?;
            step.finish_success_with(&format!("{}", package_path.display()));
        }
    }

    Ok(())
}

fn pack_wasm(config: &Config, options: PackWasmOptions, reporter: &Reporter) -> Result<()> {
    if !config.is_wasm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.enabled = false".to_string(),
            status: None,
        });
    }
    if config.wasm_npm_generate_package_json() && config.wasm_npm_package_name().is_none() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.npm.package_name is required for pack wasm".to_string(),
            status: None,
        });
    }

    reporter.section("🌐", "Packing WASM");

    let requested_wasm_profile = if options.release {
        WasmProfile::Release
    } else {
        config.wasm_profile()
    };

    let build_cargo_args = resolve_build_cargo_args(config, &options.cargo_args);
    let build_profile = resolve_build_profile(
        matches!(requested_wasm_profile, WasmProfile::Release),
        &build_cargo_args,
    );

    let wasm_artifact_profile = match build_profile {
        CargoBuildProfile::Debug => WasmProfile::Debug,
        CargoBuildProfile::Release => WasmProfile::Release,
        CargoBuildProfile::Named(_) if config.wasm_has_artifact_path_override() => {
            requested_wasm_profile
        }
        CargoBuildProfile::Named(profile_name) => {
            return Err(CliError::CommandFailed {
                command: format!(
                    "custom cargo profile '{}' for wasm pack requires targets.wasm.artifact_path",
                    profile_name
                ),
                status: None,
            });
        }
    };

    if !options.no_build {
        let step = reporter.step("Building WASM target");
        build_wasm_target(config, requested_wasm_profile, &build_cargo_args, &step)?;
        step.finish_success();
    }

    let wasm_artifact_path = config.wasm_artifact_path(wasm_artifact_profile);
    if !wasm_artifact_path.exists() {
        return Err(CliError::FileNotFound(wasm_artifact_path));
    }

    if config.wasm_optimize_enabled(wasm_artifact_profile) {
        let step = reporter.step("Optimizing WASM binary");
        optimize_wasm_binary(config, &wasm_artifact_path)?;
        step.finish_success();
    }

    if options.regenerate {
        let step = reporter.step("Generating TypeScript bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Typescript,
                output: Some(config.wasm_typescript_output()),
                experimental: false,
            },
        )?;
        step.finish_success();
    }

    let npm_output = config.wasm_npm_output();
    std::fs::create_dir_all(&npm_output).map_err(|source| CliError::CreateDirectoryFailed {
        path: npm_output.clone(),
        source,
    })?;

    let module_name = config.wasm_typescript_module_name();
    let packaged_wasm_path = npm_output.join(format!("{}_bg.wasm", module_name));
    std::fs::copy(&wasm_artifact_path, &packaged_wasm_path).map_err(|source| {
        CliError::CopyFailed {
            from: wasm_artifact_path.clone(),
            to: packaged_wasm_path.clone(),
            source,
        }
    })?;

    let generated_typescript_source = config
        .wasm_typescript_output()
        .join(format!("{}.ts", module_name));
    if !generated_typescript_source.exists() {
        return Err(CliError::FileNotFound(generated_typescript_source));
    }

    let step = reporter.step("Transpiling TypeScript bindings");
    transpile_typescript_bundle(config, &generated_typescript_source, &npm_output)?;
    step.finish_success();

    let generated_node_typescript_source = config
        .wasm_typescript_output()
        .join(format!("{}_node.ts", module_name));
    if generated_node_typescript_source.exists() {
        let step = reporter.step("Transpiling Node.js bindings");
        transpile_typescript_bundle(config, &generated_node_typescript_source, &npm_output)?;
        step.finish_success();
    }

    let enabled_targets = config.wasm_npm_targets();
    let step = reporter.step("Generating WASM loader entrypoints");
    generate_wasm_loader_entrypoints(&module_name, &enabled_targets, &npm_output)?;
    step.finish_success();

    if config.wasm_npm_generate_package_json() {
        let step = reporter.step("Generating package.json");
        let package_json_path =
            generate_wasm_package_json(config, &module_name, &enabled_targets, &npm_output)?;
        step.finish_success_with(&format!("{}", package_json_path.display()));
    }

    if config.wasm_npm_generate_readme() {
        let step = reporter.step("Generating README.md");
        let readme_path =
            generate_wasm_readme(config, &module_name, &enabled_targets, &npm_output)?;
        step.finish_success_with(&format!("{}", readme_path.display()));
    }

    Ok(())
}

fn pack_android(config: &Config, options: PackAndroidOptions, reporter: &Reporter) -> Result<()> {
    if !config.is_android_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.android.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🤖", "Packing Android");

    let build_cargo_args = resolve_build_cargo_args(config, &options.cargo_args);
    let build_profile = resolve_build_profile(options.release, &build_cargo_args);

    if !options.no_build {
        let step = reporter.step("Building Android targets");
        build_android_targets(config, options.release, &build_cargo_args, &step)?;
        step.finish_success();
    }

    if options.regenerate {
        let step = reporter.step("Generating Kotlin bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Kotlin,
                output: Some(config.android_kotlin_output()),
                experimental: false,
            },
        )?;
        step.finish_success();

        let step = reporter.step("Generating C header");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Header,
                output: Some(config.android_header_output()),
                experimental: false,
            },
        )?;
        step.finish_success();
    }

    let libraries = discover_built_libraries(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
    )?;
    let android_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform() == Platform::Android)
        .collect();

    if android_libraries.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "Android".to_string(),
        });
    }

    let packager = AndroidPackager::new(config, android_libraries, build_profile.is_release_like());
    let step = reporter.step("Packaging jniLibs");
    packager.package()?;
    step.finish_success();

    Ok(())
}

fn pack_java(config: &Config, options: PackJavaOptions, reporter: &Reporter) -> Result<()> {
    if !config.is_java_jvm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.java.jvm.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("☕", "Packing Java");

    let build_cargo_args = resolve_build_cargo_args(config, &options.cargo_args);
    let build_profile = resolve_build_profile(options.release, &build_cargo_args);

    if !options.no_build {
        let step = reporter.step("Building Rust cdylib");
        build_jvm_native_library(config, options.release, &build_cargo_args, &step)?;
        step.finish_success();
    }

    if options.regenerate {
        let step = reporter.step("Generating C header");
        generate_java_header(config)?;
        step.finish_success();

        let step = reporter.step("Generating Java bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Java,
                output: Some(config.java_jvm_output()),
                experimental: true,
            },
        )?;
        step.finish_success();
    }

    let step = reporter.step("Compiling JNI library");
    compile_jni_library(config, build_profile.output_directory_name())?;
    step.finish_success();

    reporter.finish();
    Ok(())
}

fn generate_java_header(config: &Config) -> Result<()> {
    use boltffi_bindgen::{CHeaderLowerer, ScanFeatures, ir, scan_crate_with_options};

    let output_dir = config.java_jvm_output().join("jni");
    let output_path = output_dir.join(format!("{}.h", config.library_name()));

    std::fs::create_dir_all(&output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.clone(),
        source,
    })?;

    let crate_dir = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."));
    let crate_name = config.library_name();

    let host_pointer_width_bits = match usize::BITS {
        32 => Some(32),
        64 => Some(64),
        _ => None,
    };
    let mut module = scan_crate_with_options(
        &crate_dir,
        crate_name,
        host_pointer_width_bits,
        ScanFeatures {
            record_methods: config.record_methods_enabled(),
        },
    )
    .map_err(|error| CliError::CommandFailed {
        command: format!("scan_crate: {}", error),
        status: None,
    })?;

    let contract = ir::build_contract(&mut module);
    let abi = ir::Lowerer::new(&contract).to_abi_contract();
    let header_code = CHeaderLowerer::new(&contract, &abi).generate();
    std::fs::write(&output_path, header_code).map_err(|source| CliError::WriteFailed {
        path: output_path,
        source,
    })?;

    Ok(())
}

fn compile_jni_library(config: &Config, profile_directory_name: &str) -> Result<()> {
    let java_output = config.java_jvm_output();
    let jni_dir = java_output.join("jni");
    let jni_glue = jni_dir.join("jni_glue.c");
    let header = jni_dir.join(format!("{}.h", config.library_name()));

    if !jni_glue.exists() {
        return Err(CliError::FileNotFound(jni_glue));
    }
    if !header.exists() {
        return Err(CliError::FileNotFound(header));
    }

    let artifact_name = config.library_name().replace('-', "_");
    let (lib_prefix, lib_ext, jni_platform, rpath_flag) = platform_lib_config()?;

    let rust_lib = PathBuf::from("target")
        .join(profile_directory_name)
        .join(format!("{}{}.{}", lib_prefix, artifact_name, lib_ext));

    if !rust_lib.exists() {
        return Err(CliError::FileNotFound(rust_lib));
    }

    let output_lib = java_output.join(format!("{}{}_jni.{}", lib_prefix, artifact_name, lib_ext));

    let java_home = std::env::var("JAVA_HOME").map_err(|_| CliError::CommandFailed {
        command: "JAVA_HOME not set".to_string(),
        status: None,
    })?;

    let mut cmd = Command::new("clang");
    cmd.arg("-shared")
        .arg("-fPIC")
        .arg("-o")
        .arg(&output_lib)
        .arg(&jni_glue)
        .arg(&rust_lib)
        .arg(format!("-I{}", jni_dir.display()))
        .arg(format!("-I{}/include", java_home))
        .arg(format!("-I{}/include/{}", java_home, jni_platform));

    if let Some(rpath) = rpath_flag {
        cmd.arg(rpath);
    }

    let status = cmd.status().map_err(|e| CliError::CommandFailed {
        command: format!("clang: {}", e),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "clang failed to compile JNI library".to_string(),
            status: status.code(),
        });
    }

    let dest_lib = java_output.join(format!("{}{}.{}", lib_prefix, artifact_name, lib_ext));
    std::fs::copy(&rust_lib, &dest_lib).map_err(|e| CliError::CopyFailed {
        from: rust_lib,
        to: dest_lib,
        source: e,
    })?;

    Ok(())
}

fn platform_lib_config() -> Result<(
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
)> {
    if cfg!(target_os = "macos") {
        Ok(("lib", "dylib", "darwin", Some("-Wl,-rpath,@loader_path")))
    } else if cfg!(target_os = "linux") {
        Ok(("lib", "so", "linux", Some("-Wl,-rpath,$ORIGIN")))
    } else if cfg!(target_os = "windows") {
        Ok(("", "dll", "win32", None))
    } else {
        Err(CliError::CommandFailed {
            command: "unsupported platform for JNI compilation".to_string(),
            status: None,
        })
    }
}

fn build_jvm_native_library(
    config: &Config,
    release: bool,
    build_cargo_args: &[String],
    step: &Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(print_cargo_line))
    } else {
        None
    };

    let options = BuildOptions {
        release,
        package: None,
        cargo_args: build_cargo_args.to_vec(),
        on_output,
    };

    let builder = Builder::new(config, options);
    let result = builder.build_host()?;

    if !result.success {
        return Err(CliError::BuildFailed {
            targets: vec!["host".to_string()],
        });
    }

    Ok(())
}

fn build_apple_targets(
    config: &Config,
    release: bool,
    build_cargo_args: &[String],
    step: &Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| {
            print_cargo_line(line);
        }))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
        cargo_args: build_cargo_args.to_vec(),
        on_output,
    };
    let builder = Builder::new(config, build_options);

    let mut results = builder.build_ios()?;
    if config.apple_include_macos() {
        results.extend(builder.build_macos()?);
    }

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(CliError::BuildFailed { targets: failed })
}

fn print_cargo_line(line: &str) {
    use console::style;
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("Fresh") {
        return;
    }

    if trimmed.starts_with("Compiling") {
        println!("      {}", style(trimmed).green());
    } else if trimmed.starts_with("Finished") {
        println!("      {}", style(trimmed).green().bold());
    } else if trimmed.starts_with("warning:") {
        println!("      {}", style(trimmed).yellow());
    } else if trimmed.starts_with("error") {
        println!("      {}", style(trimmed).red().bold());
    } else if trimmed.starts_with("Checking") {
        println!("      {}", style(trimmed).green());
    } else if trimmed.starts_with("Building") {
        println!("      {}", style(trimmed).cyan());
    } else {
        println!("      {}", style(trimmed).dim());
    }
}

fn build_android_targets(
    config: &Config,
    release: bool,
    build_cargo_args: &[String],
    step: &Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
        cargo_args: build_cargo_args.to_vec(),
        on_output,
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_android()?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(CliError::BuildFailed { targets: failed })
}

fn build_wasm_target(
    config: &Config,
    profile: WasmProfile,
    build_cargo_args: &[String],
    step: &Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let build_options = BuildOptions {
        release: matches!(profile, WasmProfile::Release),
        package: Some(config.library_name().to_string()),
        cargo_args: build_cargo_args.to_vec(),
        on_output,
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_wasm_with_triple(config.wasm_triple())?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(CliError::BuildFailed { targets: failed })
}

fn resolve_build_cargo_args(config: &Config, cli_cargo_args: &[String]) -> Vec<String> {
    config
        .cargo_args_for_command("build")
        .into_iter()
        .chain(cli_cargo_args.iter().cloned())
        .collect()
}

fn generate_apple_bindings(config: &Config, layout: SpmLayout, package_root: &Path) -> Result<()> {
    let swift_output_dir = match layout {
        SpmLayout::Bundled => config
            .apple_spm_wrapper_sources()
            .map(|path| package_root.join(path).join("BoltFFI"))
            .unwrap_or_else(|| package_root.join("Sources").join("BoltFFI")),
        SpmLayout::FfiOnly => package_root.join("Sources").join("BoltFFI"),
        SpmLayout::Split => config.apple_swift_output().join("BoltFFI"),
    };

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Swift,
            output: Some(swift_output_dir),
            experimental: false,
        },
    )?;

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Header,
            output: Some(config.apple_header_output()),
            experimental: false,
        },
    )?;

    Ok(())
}

fn optimize_wasm_binary(config: &Config, wasm_path: &Path) -> Result<()> {
    let optimize_level_flag = match config.wasm_optimize_level() {
        WasmOptimizeLevel::O0 => "-O0",
        WasmOptimizeLevel::O1 => "-O1",
        WasmOptimizeLevel::O2 => "-O2",
        WasmOptimizeLevel::O3 => "-O3",
        WasmOptimizeLevel::O4 => "-O4",
        WasmOptimizeLevel::Size => "-Os",
        WasmOptimizeLevel::MinSize => "-Oz",
    };

    let wasm_opt_path = match which::which("wasm-opt") {
        Ok(path) => path,
        Err(_) => {
            return match config.wasm_optimize_on_missing() {
                WasmOptimizeOnMissing::Error => Err(CliError::CommandFailed {
                    command: "wasm-opt not found in PATH".to_string(),
                    status: None,
                }),
                WasmOptimizeOnMissing::Warn => {
                    println!("warning: wasm-opt not found, skipping optimization");
                    Ok(())
                }
                WasmOptimizeOnMissing::Skip => Ok(()),
            };
        }
    };

    let optimized_path = wasm_path.with_extension("optimized.wasm");
    let mut command = Command::new(wasm_opt_path);
    command
        .arg(optimize_level_flag)
        .arg(wasm_path)
        .arg("-o")
        .arg(&optimized_path);

    if !config.wasm_optimize_strip_debug() {
        command.arg("-g");
    }

    let status = command.status().map_err(|_| CliError::CommandFailed {
        command: "wasm-opt".to_string(),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "wasm-opt".to_string(),
            status: status.code(),
        });
    }

    std::fs::rename(&optimized_path, wasm_path).map_err(|source| CliError::WriteFailed {
        path: wasm_path.to_path_buf(),
        source,
    })
}

fn transpile_typescript_bundle(
    config: &Config,
    source_file: &Path,
    output_dir: &Path,
) -> Result<()> {
    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "npx", "tsc"]);
        cmd
    } else {
        Command::new("tsc")
    };
    command
        .arg(source_file)
        .arg("--target")
        .arg("ES2020")
        .arg("--module")
        .arg("ES2020")
        .arg("--moduleResolution")
        .arg("bundler")
        .arg("--declaration")
        .arg("--sourceMap")
        .arg(if config.wasm_source_map_enabled() {
            "true"
        } else {
            "false"
        })
        .arg("--skipLibCheck")
        .arg("--noEmitOnError")
        .arg("false")
        .arg("--outDir")
        .arg(output_dir);

    let output = command.output().map_err(|_| CliError::CommandFailed {
        command: "tsc".to_string(),
        status: None,
    })?;

    let module_name = config.wasm_typescript_module_name();
    let javascript_path = output_dir.join(format!("{}.js", module_name));
    let declarations_path = output_dir.join(format!("{}.d.ts", module_name));
    let emitted_outputs_exist = javascript_path.exists() && declarations_path.exists();

    if output.status.success() || emitted_outputs_exist {
        return Ok(());
    }

    Err(CliError::CommandFailed {
        command: format!("tsc failed: {}", String::from_utf8_lossy(&output.stderr)),
        status: output.status.code(),
    })
}

fn generate_wasm_loader_entrypoints(
    module_name: &str,
    enabled_targets: &[WasmNpmTarget],
    output_dir: &Path,
) -> Result<()> {
    enabled_targets
        .iter()
        .try_for_each(|target| {
            let (filename, content) = match target {
                WasmNpmTarget::Bundler => (
                    "bundler.js",
                    format!(
                        "import init from \"./{module}.js\";\nexport * from \"./{module}.js\";\nexport {{ default as init }} from \"./{module}.js\";\nexport const initialized = (async () => {{\n  const response = await fetch(new URL(\"./{module}_bg.wasm\", import.meta.url));\n  await init(response);\n}})();\n",
                        module = module_name
                    ),
                ),
                WasmNpmTarget::Web => (
                    "web.js",
                    format!(
                        "import init from \"./{module}.js\";\nexport * from \"./{module}.js\";\nexport {{ default as init }} from \"./{module}.js\";\nexport const initialized = (async () => {{\n  const response = await fetch(new URL(\"./{module}_bg.wasm\", import.meta.url));\n  await init(response);\n}})();\n",
                        module = module_name
                    ),
                ),
                WasmNpmTarget::Nodejs => (
                    "node.js",
                    format!(
                        "export * from \"./{module}_node.js\";\nexport {{ default, initialized }} from \"./{module}_node.js\";\n",
                        module = module_name
                    ),
                ),
            };

            let path = output_dir.join(filename);
            std::fs::write(&path, content).map_err(|source| CliError::WriteFailed {
                path,
                source,
            })
        })
}

fn generate_wasm_package_json(
    config: &Config,
    module_name: &str,
    enabled_targets: &[WasmNpmTarget],
    output_dir: &Path,
) -> Result<PathBuf> {
    let package_name = config
        .wasm_npm_package_name()
        .ok_or_else(|| CliError::CommandFailed {
            command: "targets.wasm.npm.package_name is required for pack wasm".to_string(),
            status: None,
        })?;
    let package_version = config
        .wasm_npm_version()
        .unwrap_or_else(|| "0.1.0".to_string());

    let has_bundler = enabled_targets.contains(&WasmNpmTarget::Bundler);
    let has_web = enabled_targets.contains(&WasmNpmTarget::Web);
    let has_node = enabled_targets.contains(&WasmNpmTarget::Nodejs);
    let default_entry = if has_bundler {
        "./bundler.js"
    } else if has_web {
        "./web.js"
    } else {
        "./node.js"
    };

    let runtime_package = config.wasm_runtime_package();
    let runtime_version = config.wasm_runtime_version();
    let mut dependencies = BTreeMap::new();
    dependencies.insert(runtime_package, runtime_version);

    let package_json = WasmPackageJson {
        name: package_name.to_string(),
        version: package_version,
        package_type: "module".to_string(),
        exports: WasmPackageExports {
            root: WasmPackageEntry {
                types: format!("./{}.d.ts", module_name),
                browser: has_web.then(|| "./web.js".to_string()),
                node: has_node.then(|| "./node.js".to_string()),
                default: default_entry.to_string(),
            },
        },
        types: format!("./{}.d.ts", module_name),
        files: vec![
            format!("{}.js", module_name),
            format!("{}.d.ts", module_name),
            format!("{}_bg.wasm", module_name),
            "bundler.js".to_string(),
            "web.js".to_string(),
            "node.js".to_string(),
        ],
        dependencies,
        license: config.wasm_npm_license(),
        repository: config.wasm_npm_repository(),
    };

    let rendered =
        serde_json::to_string_pretty(&package_json).map_err(|source| CliError::CommandFailed {
            command: format!("failed to serialize package.json: {}", source),
            status: None,
        })?;
    let package_json_path = output_dir.join("package.json");
    std::fs::write(&package_json_path, rendered).map_err(|source| CliError::WriteFailed {
        path: package_json_path.clone(),
        source,
    })?;

    Ok(package_json_path)
}

#[derive(Serialize)]
struct WasmPackageJson {
    name: String,
    version: String,
    #[serde(rename = "type")]
    package_type: String,
    exports: WasmPackageExports,
    types: String,
    files: Vec<String>,
    dependencies: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
}

#[derive(Serialize)]
struct WasmPackageExports {
    #[serde(rename = ".")]
    root: WasmPackageEntry,
}

#[derive(Serialize)]
struct WasmPackageEntry {
    types: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node: Option<String>,
    default: String,
}

fn generate_wasm_readme(
    config: &Config,
    module_name: &str,
    enabled_targets: &[WasmNpmTarget],
    output_dir: &Path,
) -> Result<PathBuf> {
    let package_name = config.wasm_npm_package_name().unwrap_or(module_name);
    let targets_text = enabled_targets
        .iter()
        .map(|target| match target {
            WasmNpmTarget::Bundler => "bundler",
            WasmNpmTarget::Web => "web",
            WasmNpmTarget::Nodejs => "nodejs",
        })
        .collect::<Vec<_>>()
        .join(", ");
    let content = format!(
        "# {package_name}\n\nGenerated by boltffi.\n\nEnabled wasm npm targets: {targets_text}\n\n```ts\nimport {{ initialized }} from \"{package_name}\";\nawait initialized;\n```\n"
    );

    let readme_path = output_dir.join("README.md");
    std::fs::write(&readme_path, content).map_err(|source| CliError::WriteFailed {
        path: readme_path.clone(),
        source,
    })?;

    Ok(readme_path)
}

#[derive(Deserialize)]
struct CargoMetadataTargetDirectory {
    target_directory: PathBuf,
}

fn discover_built_libraries(
    crate_artifact_name: &str,
    profile_directory_name: &str,
) -> Result<Vec<BuiltLibrary>> {
    let target_directory = cargo_target_directory()?;
    Ok(BuiltLibrary::discover_for_profile(
        &target_directory,
        crate_artifact_name,
        profile_directory_name,
    ))
}

fn cargo_target_directory() -> Result<PathBuf> {
    let crate_dir = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;
    let output = Command::new("cargo")
        .current_dir(&crate_dir)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|source| CliError::CommandFailed {
            command: format!("cargo metadata: {source}"),
            status: None,
        })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: "cargo metadata --format-version 1 --no-deps".to_string(),
            status: output.status.code(),
        });
    }

    parse_cargo_target_directory(&output.stdout)
}

fn parse_cargo_target_directory(metadata: &[u8]) -> Result<PathBuf> {
    serde_json::from_slice::<CargoMetadataTargetDirectory>(metadata)
        .map(|parsed| parsed.target_directory)
        .map_err(|source| CliError::CommandFailed {
            command: format!("parse cargo metadata target_directory: {source}"),
            status: None,
        })
}

fn existing_xcframework_checksum(config: &Config) -> Result<String> {
    let xcframework_zip = config
        .apple_xcframework_output()
        .join(format!("{}.xcframework.zip", config.xcframework_name()));

    if xcframework_zip.exists() {
        return compute_checksum(&xcframework_zip);
    }

    Err(CliError::FileNotFound(xcframework_zip))
}

fn detect_version() -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("version = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string())
                })
        })
}

#[cfg(test)]
mod tests {
    use super::parse_cargo_target_directory;
    use std::path::PathBuf;

    #[test]
    fn parses_target_directory_from_cargo_metadata() {
        let metadata = br#"{
            "packages": [],
            "workspace_members": [],
            "workspace_default_members": [],
            "resolve": null,
            "target_directory": "/tmp/boltffi-target",
            "version": 1,
            "workspace_root": "/tmp/demo"
        }"#;

        let target_directory =
            parse_cargo_target_directory(metadata).expect("expected target directory");

        assert_eq!(target_directory, PathBuf::from("/tmp/boltffi-target"));
    }
}
