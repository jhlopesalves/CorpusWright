use corpusaid_core::{CleaningConfig, ExportOptions, export_corpus, scan_directory};
use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input_directory> <output_directory>", args[0]);
        process::exit(1);
    }

    let input_dir = &args[1];
    let output_dir = PathBuf::from(&args[2]);
    println!("Scanning directory: {}", input_dir);

    match scan_directory(input_dir) {
        Ok(report) => {
            if report.files.is_empty() {
                eprintln!("No supported files found. Aborting export.");
                process::exit(1);
            }

            println!(
                "Found {} supported files. Exporting...",
                report.files_supported
            );

            let config = CleaningConfig {
                lowercase: true,
                trim_lines: true,
                ..CleaningConfig::default()
            };

            let options = ExportOptions {
                app_name: "CorpusWright Example".to_string(),
                app_version: Some("0.1.0".to_string()),
                overwrite: true,
            };

            match export_corpus(
                &report.files,
                output_dir.clone(),
                &config,
                &options,
                None,
                None,
            ) {
                Ok(export) => {
                    println!("\nExport successful!");
                    println!("  Files exported: {}", export.files_exported);
                    println!("  Output directory: {}", output_dir.display());
                    println!("  Manifest path: {}", export.manifest_path.display());
                }
                Err(e) => {
                    eprintln!("Error exporting corpus: {:?}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error scanning directory: {:?}", e);
            process::exit(1);
        }
    }
}
