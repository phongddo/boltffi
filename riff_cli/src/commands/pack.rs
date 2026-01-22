use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::build::{BuildOptions, Builder, all_successful, failed_targets};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::config::{Config, SpmDistribution, SpmLayout};
use crate::error::{CliError, Result};
use crate::pack::{AndroidPackager, SpmPackageGenerator, XcframeworkBuilder, compute_checksum};
use crate::target::{BuiltLibrary, Platform};

pub enum PackCommand {
    Apple(PackAppleOptions),
    Android(PackAndroidOptions),
}

pub struct PackAppleOptions {
    pub release: bool,
    pub version: Option<String>,
    pub regenerate: bool,
    pub no_build: bool,
    pub spm_only: bool,
    pub xcframework_only: bool,
    pub layout: Option<SpmLayout>,
}

pub struct PackAndroidOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
}

pub fn run_pack(config: &Config, command: PackCommand) -> Result<()> {
    match command {
        PackCommand::Apple(options) => pack_apple(config, options),
        PackCommand::Android(options) => pack_android(config, options),
    }
}

fn pack_apple(config: &Config, options: PackAppleOptions) -> Result<()> {
    if options.spm_only && options.xcframework_only {
        return Err(CliError::CommandFailed {
            command: "cannot combine --spm-only and --xcframework-only".to_string(),
            status: None,
        });
    }

    if !options.no_build {
        run_step("Building Apple targets", || build_apple_targets(config, options.release))?;
    }

    let layout = options.layout.unwrap_or_else(|| config.apple_spm_layout());
    let package_root = config.apple_spm_output();

    if options.regenerate {
        run_step("Generating Apple bindings", || {
            generate_apple_bindings(config, layout, &package_root)
        })?;
    }

    let libraries = discover_built_libraries(config.library_name(), options.release);
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

    let xcframework_output = should_build_xcframework
        .then(|| {
            run_step("Creating xcframework", || {
                XcframeworkBuilder::new(config, apple_libraries.clone(), headers_dir.clone())
                    .build_with_zip()
            })
        })
        .transpose()?;

    if should_generate_spm {
        let (checksum, version) = match config.apple_spm_distribution() {
            SpmDistribution::Local => (None, None),
            SpmDistribution::Remote => {
                let checksum = xcframework_output
                    .as_ref()
                    .and_then(|o| o.checksum.clone())
                    .map(Ok)
                    .unwrap_or_else(|| {
                        run_step("Computing checksum from existing xcframework.zip", || {
                            existing_xcframework_checksum(config)
                        })
                    })?;
                let version =
                    options.version.or_else(detect_version).unwrap_or_else(|| "0.1.0".to_string());
                (Some(checksum), Some(version))
            }
        };

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

        let package_path = run_step("Generating Package.swift", || generator.generate())?;
        println!("Created: {}", package_path.display());
    }

    if let Some(output) = xcframework_output {
        println!("Created: {}", output.xcframework_path.display());
        output
            .zip_path
            .as_ref()
            .iter()
            .for_each(|path| println!("Created: {}", path.display()));
        output
            .checksum
            .as_ref()
            .iter()
            .for_each(|checksum| println!("Checksum: {}", checksum));
    }

    Ok(())
}

fn pack_android(config: &Config, options: PackAndroidOptions) -> Result<()> {
    if !options.no_build {
        run_step("Building Android targets", || build_android_targets(config, options.release))?;
    }

    if options.regenerate {
        run_step("Generating Kotlin bindings", || {
            run_generate_with_output(
                config,
                GenerateOptions {
                    target: GenerateTarget::Kotlin,
                    output: Some(config.android_kotlin_output()),
                },
            )
        })?;
        run_step("Generating C header", || {
            run_generate_with_output(
                config,
                GenerateOptions {
                    target: GenerateTarget::Header,
                    output: Some(config.android_header_output()),
                },
            )
        })?;
    }

    let libraries = discover_built_libraries(config.library_name(), options.release);
    let android_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|lib| lib.target.platform() == Platform::Android)
        .collect();

    if android_libraries.is_empty() {
        return Err(CliError::NoLibrariesFound {
            platform: "Android".to_string(),
        });
    }

    let packager = AndroidPackager::new(config, android_libraries, options.release);
    let output = run_step("Packaging jniLibs", || packager.package())?;

    println!("Created: {}", output.jnilibs_path.display());
    output
        .copied_libraries
        .iter()
        .for_each(|path| println!("  {}", path.display()));

    Ok(())
}

fn build_apple_targets(config: &Config, release: bool) -> Result<()> {
    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
    };
    let builder = Builder::new(config, build_options);

    let mut results = builder.build_ios()?;
    if config.apple.include_macos {
        results.extend(builder.build_macos()?);
    }

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results)
        .iter()
        .map(|target| target.triple().to_string())
        .collect::<Vec<_>>();

    Err(CliError::BuildFailed { targets: failed })
}

fn build_android_targets(config: &Config, release: bool) -> Result<()> {
    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_android()?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results)
        .iter()
        .map(|target| target.triple().to_string())
        .collect::<Vec<_>>();

    Err(CliError::BuildFailed { targets: failed })
}

fn generate_apple_bindings(config: &Config, layout: SpmLayout, package_root: &Path) -> Result<()> {
    let swift_output_dir = match layout {
        SpmLayout::Bundled => config
            .apple_spm_wrapper_sources()
            .map(|path| package_root.join(path).join("Riff"))
            .unwrap_or_else(|| package_root.join("Sources").join("Riff")),
        SpmLayout::FfiOnly => package_root.join("Sources").join("Riff"),
        SpmLayout::Split => config.apple_swift_output().join("Riff"),
    };

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Swift,
            output: Some(swift_output_dir),
        },
    )?;

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Header,
            output: Some(config.apple_header_output()),
        },
    )?;

    Ok(())
}

fn discover_built_libraries(library_name: &str, release: bool) -> Vec<BuiltLibrary> {
    BuiltLibrary::discover(&PathBuf::from("target"), library_name, release)
}

fn existing_xcframework_checksum(config: &Config) -> Result<String> {
    let xcframework_zip = config.apple_xcframework_output().join(format!(
        "{}.xcframework.zip",
        config.xcframework_name()
    ));

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

fn run_step<T>(label: &str, action: impl FnOnce() -> Result<T>) -> Result<T> {
    print!("{}... ", label);
    io::stdout().flush().ok();
    action().map(|value| {
        println!("✓");
        value
    })
}
