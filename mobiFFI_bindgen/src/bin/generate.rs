use std::env;
use std::fs;
use std::path::PathBuf;

use mobiFFI_bindgen::model::Module;
use mobiFFI_bindgen::{scan_crate, Swift};

fn generate_swift(module: &Module) -> String {
    let mut output = String::new();

    output.push_str("import Foundation\n\n");

    for class in &module.classes {
        let wrappers = Swift::render_stream_wrappers(class, module);
        if !wrappers.is_empty() {
            output.push_str(&wrappers);
            output.push_str("\n\n");
        }
        output.push_str(&Swift::render_class(class, module));
        output.push_str("\n\n");
    }

    output
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (crate_path, output_path) = if args.len() >= 3 {
        (PathBuf::from(&args[1]), PathBuf::from(&args[2]))
    } else {
        (
            PathBuf::from("mobiFFI_core"),
            PathBuf::from("swift_test/Generated.swift"),
        )
    };

    let module_name = crate_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("core");

    println!("Scanning crate: {}", crate_path.display());

    let module = match scan_crate(&crate_path, module_name) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error scanning crate: {}", e);
            std::process::exit(1);
        }
    };

    println!(
        "Found {} classes, {} records",
        module.classes.len(),
        module.records.len()
    );

    let metadata_dir = PathBuf::from("target/mobiFFI");
    fs::create_dir_all(&metadata_dir).expect("Failed to create metadata directory");
    let metadata_path = metadata_dir.join(format!("{}.json", module_name));
    let metadata_json =
        serde_json::to_string_pretty(&module).expect("Failed to serialize module to JSON");
    fs::write(&metadata_path, &metadata_json).expect("Failed to write metadata JSON");
    println!("Metadata written to: {}", metadata_path.display());

    let swift_code = generate_swift(&module);
    fs::write(&output_path, &swift_code).expect("Failed to write generated Swift file");
    println!("Swift code written to: {}", output_path.display());

    println!("\n--- Classes ---");
    for class in &module.classes {
        println!(
            "  {} ({} methods, {} streams)",
            class.name,
            class.methods.len(),
            class.streams.len()
        );
    }

    println!("\n--- Records ---");
    for record in &module.records {
        println!("  {} ({} fields)", record.name, record.fields.len());
    }
}
