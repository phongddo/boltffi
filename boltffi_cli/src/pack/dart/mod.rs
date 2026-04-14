use crate::{
    build::{
        BuildOptions, Builder, CargoBuildProfile, OutputCallback, all_successful, failed_targets,
        resolve_build_profile,
    },
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::PackDartOptions,
    },
    config::Config,
    pack::{
        PackError, discover_built_libraries_for_targets, print_cargo_line, resolve_build_cargo_args,
    },
    reporter::{Reporter, Step},
};

fn build_dart_targets(
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
    let results = builder.build_targets(&config.dart_targets())?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(CliError::Pack(PackError::BuildFailed { targets: failed }))
}

pub(crate) fn pack_dart(
    config: &Config,
    options: PackDartOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_dart_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.dart.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("☕", "Packing Dart");

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);

    if !options.execution.no_build {
        let step = reporter.step("Building Rust cdylib");
        build_dart_targets(
            config,
            matches!(build_profile, CargoBuildProfile::Release),
            &build_cargo_args,
            &step,
        )?;
        step.finish_success();
    }

    if options.execution.regenerate {
        let step = reporter.step("Generating Dart bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Dart,
                output: Some(config.dart_output()),
                experimental: options.experimental,
            },
        )?;

        step.finish_success();
    }

    let step = reporter.step("Packaging native libraries");

    let libraries = discover_built_libraries_for_targets(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
        &config.dart_targets(),
    )?;

    let package_dir = config.dart_output().join(&config.package.name);

    let native_libs_dir = package_dir.join("native");
    std::fs::create_dir_all(&native_libs_dir).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: native_libs_dir.clone(),
            source,
        }
    })?;
    for l in libraries {
        let lib_triple_dir = native_libs_dir.join(l.target.triple());
        std::fs::create_dir_all(&lib_triple_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: lib_triple_dir.clone(),
                source,
            }
        })?;
        std::fs::copy(
            &l.path,
            lib_triple_dir.join(l.path.file_name().expect("file shouldn't terminate in ..")),
        )
        .map_err(|source| CliError::CopyFailed {
            from: l.path,
            to: lib_triple_dir,
            source,
        })?;
    }
    step.finish_success();

    reporter.finish();
    Ok(())
}
