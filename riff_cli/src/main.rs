mod android;
mod build;
mod check;
mod commands;
mod config;
mod error;
mod pack;
mod target;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use commands::build::{BuildCommandOptions, BuildPlatform};
use commands::check::CheckOptions;
use commands::doctor::DoctorOptions;
use commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use commands::init::InitOptions;
use commands::pack::{PackAndroidOptions, PackAppleOptions, PackCommand};
use commands::verify::VerifyOptions;
use commands::{run_build, run_check, run_doctor, run_init, run_pack, run_verify};
use config::Config;
use error::{CliError, Result};

#[derive(Parser)]
#[command(name = "riff")]
#[command(about = "Riff - Rust FFI toolchain (Apple + Android)")]
#[command(
    after_help = "Examples:\n  riff init\n  riff check --apple\n  riff generate swift\n  riff build apple --release\n  riff pack apple --layout bundled\n\nConfig:\n  riff reads ./riff.toml\n  Apple settings live under [apple.*]\n  Android settings live under [android.*]\n"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Create riff.toml with sensible defaults")]
    Init {
        #[arg(long)]
        name: Option<String>,
    },

    #[command(
        about = "Check toolchain + required rust targets",
        long_about = "Check toolchain + required rust targets.\n\nIf no platform flags are provided, riff checks both Apple and Android.\n\nExamples:\n  riff check\n  riff check --apple\n  riff check --android\n  riff check --fix\n"
    )]
    Check {
        #[arg(long, help = "Install missing rust targets")]
        fix: bool,

        #[arg(long, help = "Check Apple (iOS/iOS-simulator) targets + Xcode tooling")]
        apple: bool,

        #[arg(long, help = "Check Android targets + NDK")]
        android: bool,
    },

    #[command(
        about = "Print diagnostic environment info",
        long_about = "Print diagnostic environment info.\n\nExamples:\n  riff doctor\n  riff doctor --apple\n  riff doctor --android\n"
    )]
    Doctor {
        #[arg(long)]
        apple: bool,

        #[arg(long)]
        android: bool,
    },

    #[command(
        about = "Generate bindings (Swift/Kotlin/header)",
        long_about = "Generate bindings.\n\nExamples:\n  riff generate\n  riff generate swift\n  riff generate kotlin\n  riff generate header\n"
    )]
    Generate {
        #[arg(value_enum)]
        target: Option<GenerateTargetArg>,

        #[arg(
            short,
            long,
            help = "Override output directory (default comes from riff.toml)"
        )]
        output: Option<PathBuf>,
    },

    #[command(
        about = "Build rust libraries for targets",
        long_about = "Build rust libraries for targets.\n\nExamples:\n  riff build\n  riff build apple\n  riff build android --release\n"
    )]
    Build {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,

        #[arg(long)]
        release: bool,
    },

    #[command(
        about = "Package platform artifacts (xcframework/SPM/jniLibs)",
        long_about = "Package platform artifacts.\n\nExamples:\n  riff pack apple\n  riff pack apple --layout bundled\n  riff pack android --release\n"
    )]
    Pack {
        #[command(subcommand)]
        target: PackTargetArg,
    },

    #[command(about = "Run check/build/generate/pack in order")]
    Release {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,
    },

    #[command(about = "Verify a generated binding file")]
    Verify {
        path: PathBuf,

        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum GenerateTargetArg {
    #[value(help = "Generate Swift bindings")]
    Swift,
    #[value(help = "Generate Kotlin bindings + JNI glue")]
    Kotlin,
    #[value(help = "Generate C header")]
    Header,
    #[value(help = "Generate all bindings")]
    All,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum BuildPlatformArg {
    #[value(help = "Build Apple targets (iOS + iOS-simulator, and macOS if enabled)")]
    Apple,
    #[value(help = "Build Android targets")]
    Android,
    #[value(help = "Build macOS target")]
    Macos,
    #[value(help = "Build all configured targets")]
    All,
}

#[derive(Subcommand)]
enum PackTargetArg {
    #[command(
        about = "Build + package Apple artifacts",
        long_about = "Build + package Apple artifacts.\n\nOutputs:\n  - xcframework: {apple.xcframework.output}/{Name}.xcframework\n  - SwiftPM:      {apple.spm.output}/Package.swift\n\nLayout:\n  bundled  -> one package with wrapper target\n  ffi-only -> standalone FFI package with Swift target\n  split    -> binary-only package (Swift bindings generated to apple.swift.output)\n"
    )]
    Apple {
        #[arg(long)]
        release: bool,

        #[arg(long)]
        version: Option<String>,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,

        #[arg(long)]
        spm_only: bool,

        #[arg(long)]
        xcframework_only: bool,

        #[arg(long, value_enum)]
        layout: Option<PackLayoutArg>,
    },

    #[command(
        about = "Build + package Android artifacts",
        long_about = "Build + package Android artifacts.\n\nOutputs:\n  - Kotlin/JNI: {android.kotlin.output}\n  - jniLibs:    {android.pack.output}\n"
    )]
    Android {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PackLayoutArg {
    #[value(help = "Single SwiftPM package with wrapper target + generated sources")]
    Bundled,
    #[value(help = "Binary-only SwiftPM package; generate Swift sources outside package")]
    Split,
    #[value(help = "Standalone FFI SwiftPM package (binary target + generated Swift target)")]
    FfiOnly,
}

fn main() {
    let cli = Cli::parse();

    let result = execute_command(cli.command);

    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn execute_command(command: Commands) -> Result<()> {
    match command {
        Commands::Init { name } => {
            let options = InitOptions {
                name,
                path: std::env::current_dir().unwrap_or_default(),
            };
            run_init(options).map(|_| ())
        }

        Commands::Check {
            fix,
            apple,
            android,
        } => {
            let options = CheckOptions {
                fix,
                apple: apple || !android,
                android: android || !apple,
            };
            run_check(options).map(|_| ())
        }

        Commands::Doctor { apple, android } => {
            let options = DoctorOptions {
                apple: apple || !android,
                android: android || !apple,
            };
            run_doctor(options)
        }

        Commands::Generate { target, output } => {
            let config = load_config()?;
            let options = GenerateOptions {
                target: target
                    .map(|t| match t {
                        GenerateTargetArg::Swift => GenerateTarget::Swift,
                        GenerateTargetArg::Kotlin => GenerateTarget::Kotlin,
                        GenerateTargetArg::Header => GenerateTarget::Header,
                        GenerateTargetArg::All => GenerateTarget::All,
                    })
                    .unwrap_or(GenerateTarget::All),
                output,
            };
            run_generate_with_output(&config, options)
        }

        Commands::Build { platform, release } => {
            let config = load_config()?;
            let options = BuildCommandOptions {
                platform: platform
                    .map(|p| match p {
                        BuildPlatformArg::Apple => BuildPlatform::Apple,
                        BuildPlatformArg::Android => BuildPlatform::Android,
                        BuildPlatformArg::Macos => BuildPlatform::MacOs,
                        BuildPlatformArg::All => BuildPlatform::All,
                    })
                    .unwrap_or(BuildPlatform::All),
                release,
            };
            run_build(&config, options).map(|_| ())
        }

        Commands::Pack { target } => {
            let config = load_config()?;
            let command = match target {
                PackTargetArg::Apple {
                    release,
                    version,
                    regenerate,
                    no_build,
                    spm_only,
                    xcframework_only,
                    layout,
                } => PackCommand::Apple(PackAppleOptions {
                    release,
                    version,
                    regenerate,
                    no_build,
                    spm_only,
                    xcframework_only,
                    layout: layout.map(|l| match l {
                        PackLayoutArg::Bundled => crate::config::SpmLayout::Bundled,
                        PackLayoutArg::Split => crate::config::SpmLayout::Split,
                        PackLayoutArg::FfiOnly => crate::config::SpmLayout::FfiOnly,
                    }),
                }),
                PackTargetArg::Android {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::Android(PackAndroidOptions {
                    release,
                    regenerate,
                    no_build,
                }),
            };
            run_pack(&config, command)
        }

        Commands::Release { platform } => {
            let config = load_config()?;
            run_release(&config, platform)
        }

        Commands::Verify { path, json } => {
            let options = VerifyOptions { path, json };
            run_verify(options).map(|verified| {
                if !verified {
                    std::process::exit(1);
                }
            })
        }
    }
}

fn load_config() -> Result<Config> {
    let config_path = PathBuf::from("riff.toml");

    if !config_path.exists() {
        return Err(CliError::ConfigNotFound);
    }

    Config::load(&config_path).map_err(Into::into)
}

fn run_release(config: &Config, platform: Option<BuildPlatformArg>) -> Result<()> {
    println!("Running full release pipeline...");
    println!();

    let check_options = CheckOptions {
        fix: false,
        apple: true,
        android: true,
    };

    println!("[1/4] Checking environment...");
    let env_ok = run_check(check_options)?;

    if !env_ok {
        println!("Environment check failed. Run 'riff check --fix' to install missing targets.");
        return Ok(());
    }
    println!();

    println!("[2/4] Building...");
    let build_options = BuildCommandOptions {
        platform: platform
            .map(|p| match p {
                BuildPlatformArg::Apple => BuildPlatform::Apple,
                BuildPlatformArg::Android => BuildPlatform::Android,
                BuildPlatformArg::Macos => BuildPlatform::MacOs,
                BuildPlatformArg::All => BuildPlatform::All,
            })
            .unwrap_or(BuildPlatform::All),
        release: true,
    };
    run_build(config, build_options)?;
    println!();

    println!("[3/4] Generating bindings...");
    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::All,
            output: None,
        },
    )?;
    println!();

    println!("[4/4] Packaging...");

    match platform {
        Some(BuildPlatformArg::Apple) | None => {
            run_pack(
                config,
                PackCommand::Apple(PackAppleOptions {
                    release: true,
                    version: None,
                    regenerate: false,
                    no_build: true,
                    spm_only: false,
                    xcframework_only: false,
                    layout: None,
                }),
            )?;
        }
        Some(BuildPlatformArg::Android) => {
            run_pack(
                config,
                PackCommand::Android(PackAndroidOptions {
                    release: true,
                    regenerate: false,
                    no_build: true,
                }),
            )?;
        }
        _ => {}
    }

    println!();
    println!("Release complete!");

    Ok(())
}
