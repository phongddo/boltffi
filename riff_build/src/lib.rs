use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use riff_bindgen::{scan_crate, CHeaderGenerator};

pub fn generate() {
    let crate_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let crate_name = read_crate_name(&crate_dir);
    let header_name = format!("{}.h", crate_name);

    generate_header(&crate_dir, &out_dir, &header_name, &crate_name);
    copy_header_to_dist(&crate_dir, &out_dir, &header_name);

    println!("cargo:rerun-if-changed=src/");
}

pub fn generate_with_options(options: GenerateOptions) {
    let crate_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let crate_name = options
        .crate_name
        .unwrap_or_else(|| read_crate_name(&crate_dir));
    let header_name = options
        .header_name
        .unwrap_or_else(|| format!("{}.h", crate_name));

    generate_header(&crate_dir, &out_dir, &header_name, &crate_name);

    if options.copy_to_crate {
        copy_header_to_dist(&crate_dir, &out_dir, &header_name);
    }

    println!("cargo:rerun-if-changed=src/");
}

#[derive(Default)]
pub struct GenerateOptions {
    pub crate_name: Option<String>,
    pub header_name: Option<String>,
    pub copy_to_crate: bool,
}

impl GenerateOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn crate_name(mut self, name: impl Into<String>) -> Self {
        self.crate_name = Some(name.into());
        self
    }

    pub fn header_name(mut self, name: impl Into<String>) -> Self {
        self.header_name = Some(name.into());
        self
    }

    pub fn copy_to_crate(mut self, copy: bool) -> Self {
        self.copy_to_crate = copy;
        self
    }
}

fn read_crate_name(crate_dir: &Path) -> String {
    let cargo_toml_path = crate_dir.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");

    content
        .lines()
        .find(|line| line.trim().starts_with("name"))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"').to_string())
        .expect("Failed to find package name in Cargo.toml")
}

fn generate_header(crate_dir: &Path, out_dir: &Path, header_name: &str, crate_name: &str) {
    let header_path = out_dir.join(header_name);

    let module = match scan_crate(crate_dir, crate_name) {
        Ok(m) => m,
        Err(e) => {
            println!("cargo:warning=Failed to scan crate: {}", e);
            return;
        }
    };

    let header_content = CHeaderGenerator::generate(&module);
    fs::write(&header_path, header_content).expect("Failed to write header");

    println!("cargo:warning=Generated header: {}", header_path.display());
}

fn copy_header_to_dist(crate_dir: &Path, out_dir: &Path, header_name: &str) {
    let source = out_dir.join(header_name);
    let dist_include = crate_dir.join("dist").join("include");

    if source.exists() {
        if !dist_include.exists() {
            fs::create_dir_all(&dist_include).expect("Failed to create dist/include directory");
        }
        let dest = dist_include.join(header_name);
        fs::copy(&source, &dest).expect("Failed to copy header to dist/include");
        println!("cargo:warning=Header: {}", dest.display());
    }
}
