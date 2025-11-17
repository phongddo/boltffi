use std::env;
use std::fs;
use std::path::PathBuf;

use mobiFFI_bindgen::model::Module;
use mobiFFI_bindgen::{scan_crate, Swift};

fn read_crate_name(crate_path: &PathBuf) -> String {
    let cargo_toml_path = crate_path.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");

    content
        .lines()
        .find(|line| line.starts_with("name"))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"').to_string())
        .expect("Failed to find package name in Cargo.toml")
}

fn generate_swift(module: &Module) -> String {
    let mut output = String::new();

    output.push_str(&Swift::render_preamble(module));
    output.push_str("\n\n");

    module.functions.iter().for_each(|function| {
        output.push_str(&Swift::render_function(function, module));
        output.push_str("\n\n");
    });

    module.classes.iter().for_each(|class_item| {
        let wrappers = Swift::render_stream_wrappers(class_item, module);
        if !wrappers.is_empty() {
            output.push_str(&wrappers);
            output.push_str("\n\n");
        }
        output.push_str(&Swift::render_class(class_item, module));
        output.push_str("\n\n");
    });

    module
        .callback_traits
        .iter()
        .for_each(|callback_trait_item| {
            output.push_str(&Swift::render_callback_trait(callback_trait_item, module));
            output.push_str("\n\n");
        });

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

    let module_name = read_crate_name(&crate_path);

    println!("Scanning crate: {}", crate_path.display());

    let module = match scan_crate(&crate_path, &module_name) {
        Ok(scanned_module) => scanned_module,
        Err(error) => {
            eprintln!("Error scanning crate: {}", error);
            std::process::exit(1);
        }
    };

    println!(
        "Found {} classes, {} records, {} functions",
        module.classes.len(),
        module.records.len(),
        module.functions.len()
    );

    let metadata_dir = PathBuf::from("target").join(&module_name);
    fs::create_dir_all(&metadata_dir).expect("Failed to create metadata directory");
    let metadata_path = metadata_dir.join("metadata.json");
    let metadata_json =
        serde_json::to_string_pretty(&module).expect("Failed to serialize module to JSON");
    fs::write(&metadata_path, &metadata_json).expect("Failed to write metadata JSON");
    println!("Metadata written to: {}", metadata_path.display());

    let swift_code = generate_swift(&module);
    fs::write(&output_path, &swift_code).expect("Failed to write generated Swift file");
    println!("Swift code written to: {}", output_path.display());

    println!("\n--- Classes ---");
    module.classes.iter().for_each(|class_item| {
        println!(
            "  {} ({} methods, {} streams)",
            class_item.name,
            class_item.methods.len(),
            class_item.streams.len()
        );
    });

    println!("\n--- Records ---");
    module.records.iter().for_each(|record_item| {
        println!("  {} ({} fields)", record_item.name, record_item.fields.len());
    });

    println!("\n--- Functions ---");
    module.functions.iter().for_each(|function_item| {
        println!(
            "  {} ({} params)",
            function_item.name,
            function_item.inputs.len()
        );
    });

    println!("\n--- Callback Traits ---");
    module
        .callback_traits
        .iter()
        .for_each(|callback_trait_item| {
            println!(
                "  {} ({} methods)",
                callback_trait_item.name,
                callback_trait_item.methods.len()
            );
        });
}
