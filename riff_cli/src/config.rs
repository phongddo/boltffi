use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub package: PackageConfig,
    #[serde(default)]
    pub apple: AppleConfig,
    #[serde(default)]
    pub android: AndroidConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageConfig {
    pub name: String,
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ErrorStyle {
    #[default]
    Throwing,
    Result,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FactoryStyle {
    #[default]
    Constructors,
    CompanionMethods,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AppleSwiftConfig {
    pub module_name: Option<String>,
    pub output: Option<PathBuf>,
    pub ffi_module_name: Option<String>,
    pub tools_version: Option<String>,
    #[serde(default)]
    pub error_style: ErrorStyle,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AndroidKotlinConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub module_name: Option<String>,
    pub library_name: Option<String>,
    #[serde(default)]
    pub api_style: KotlinApiStyle,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub factory_style: FactoryStyle,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KotlinApiStyle {
    #[default]
    TopLevel,
    ModuleObject,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppleConfig {
    #[serde(default = "default_apple_output")]
    pub output: PathBuf,
    #[serde(default = "default_apple_deployment_target")]
    pub deployment_target: String,
    #[serde(default)]
    pub include_macos: bool,
    #[serde(default)]
    pub swift: AppleSwiftConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub xcframework: XcframeworkConfig,
    #[serde(default)]
    pub spm: SpmConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AndroidConfig {
    #[serde(default = "default_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
    pub ndk_version: Option<String>,
    #[serde(default)]
    pub kotlin: AndroidKotlinConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub pack: AndroidPackConfig,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct HeaderConfig {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct XcframeworkConfig {
    pub output: Option<PathBuf>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpmDistribution {
    #[default]
    Local,
    Remote,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SpmConfig {
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub distribution: SpmDistribution,
    pub repo_url: Option<String>,
    #[serde(default)]
    pub layout: SpmLayout,
    pub package_name: Option<String>,
    pub wrapper_sources: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AndroidPackConfig {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SpmLayout {
    Bundled,
    Split,
    #[default]
    FfiOnly,
}

impl Default for AppleConfig {
    fn default() -> Self {
        Self {
            output: default_apple_output(),
            deployment_target: default_apple_deployment_target(),
            include_macos: false,
            swift: AppleSwiftConfig::default(),
            header: HeaderConfig::default(),
            xcframework: XcframeworkConfig::default(),
            spm: SpmConfig::default(),
        }
    }
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            output: default_android_output(),
            min_sdk: default_android_min_sdk(),
            ndk_version: None,
            kotlin: AndroidKotlinConfig::default(),
            header: HeaderConfig::default(),
            pack: AndroidPackConfig::default(),
        }
    }
}

impl Default for SpmConfig {
    fn default() -> Self {
        Self {
            output: None,
            distribution: SpmDistribution::Local,
            repo_url: None,
            layout: SpmLayout::default(),
            package_name: None,
            wrapper_sources: None,
        }
    }
}

fn default_apple_output() -> PathBuf {
    PathBuf::from("dist/apple")
}

fn default_apple_deployment_target() -> String {
    "16.0".to_string()
}

fn default_android_output() -> PathBuf {
    PathBuf::from("dist/android")
}

fn default_android_min_sdk() -> u32 {
    24
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|err| ConfigError::Read {
            path: path.to_path_buf(),
            source: err,
        })?;

        toml::from_str(&content).map_err(|err| ConfigError::Parse {
            path: path.to_path_buf(),
            source: err,
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;

        std::fs::write(path, content).map_err(|err| ConfigError::Write {
            path: path.to_path_buf(),
            source: err,
        })
    }

    pub fn library_name(&self) -> &str {
        self.package
            .crate_name
            .as_deref()
            .unwrap_or(&self.package.name)
    }

    pub fn swift_module_name(&self) -> String {
        self.apple
            .swift
            .module_name
            .clone()
            .unwrap_or_else(|| to_pascal_case(&self.package.name))
    }

    pub fn xcframework_name(&self) -> String {
        self.apple
            .xcframework
            .name
            .clone()
            .unwrap_or_else(|| self.swift_module_name())
    }

    pub fn apple_swift_output(&self) -> PathBuf {
        self.apple
            .swift
            .output
            .clone()
            .unwrap_or_else(|| self.apple.output.join("Sources"))
    }

    pub fn apple_swift_ffi_module_name(&self) -> Option<&str> {
        self.apple.swift.ffi_module_name.as_deref()
    }

    pub fn apple_header_output(&self) -> PathBuf {
        self.apple
            .header
            .output
            .clone()
            .unwrap_or_else(|| self.apple.output.join("include"))
    }

    pub fn apple_xcframework_output(&self) -> PathBuf {
        self.apple
            .xcframework
            .output
            .clone()
            .unwrap_or_else(|| self.apple.output.clone())
    }

    pub fn apple_spm_output(&self) -> PathBuf {
        self.apple
            .spm
            .output
            .clone()
            .unwrap_or_else(|| self.apple.output.clone())
    }

    pub fn apple_spm_layout(&self) -> SpmLayout {
        self.apple.spm.layout
    }

    pub fn apple_spm_wrapper_sources(&self) -> Option<&Path> {
        self.apple.spm.wrapper_sources.as_deref()
    }

    pub fn android_kotlin_package(&self) -> String {
        self.android.kotlin.package.clone().unwrap_or_else(|| {
            let normalized_name = self.package.name.replace('-', "_");
            format!("com.example.{}", normalized_name)
        })
    }

    pub fn android_kotlin_module_name(&self) -> String {
        self.android
            .kotlin
            .module_name
            .clone()
            .unwrap_or_else(|| self.kotlin_class_name())
    }

    pub fn android_kotlin_library_name(&self) -> Option<&str> {
        self.android.kotlin.library_name.as_deref()
    }

    pub fn android_jni_library_name(&self) -> String {
        self.android
            .kotlin
            .library_name
            .clone()
            .unwrap_or_else(|| format!("{}_jni", self.library_name()))
    }

    pub fn android_kotlin_output(&self) -> PathBuf {
        self.android
            .kotlin
            .output
            .clone()
            .unwrap_or_else(|| self.android.output.join("kotlin"))
    }

    pub fn android_header_output(&self) -> PathBuf {
        self.android
            .header
            .output
            .clone()
            .unwrap_or_else(|| self.android.output.join("include"))
    }

    pub fn android_pack_output(&self) -> PathBuf {
        self.android
            .pack
            .output
            .clone()
            .unwrap_or_else(|| self.android.output.join("jniLibs"))
    }

    pub fn kotlin_class_name(&self) -> String {
        to_pascal_case(&self.package.name)
    }

    pub fn apple_spm_distribution(&self) -> SpmDistribution {
        self.apple.spm.distribution
    }

    pub fn apple_spm_repo_url(&self) -> Option<&str> {
        self.apple.spm.repo_url.as_deref()
    }

    pub fn apple_swift_tools_version(&self) -> Option<&str> {
        self.apple.swift.tools_version.as_deref()
    }

    pub fn apple_spm_package_name(&self) -> Option<&str> {
        self.apple.spm.package_name.as_deref()
    }
}

fn to_pascal_case(input: &str) -> String {
    input
        .split(['_', '-'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config from {path}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config from {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to serialize config")]
    Serialize(#[source] toml::ser::Error),

    #[error("failed to write config to {path}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apple_table() {
        let cfg: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[apple]
deployment_target = "16.0"
include_macos = false
"#,
        )
        .expect("toml parse failed");

        assert_eq!(cfg.apple.deployment_target, "16.0");
        assert!(!cfg.apple.include_macos);
    }
}
