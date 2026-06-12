use corpuswright_core::{PreviewOptions, preview_files, scan_directory};
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input_directory>", args[0]);
        process::exit(1);
    }

    let input_dir = &args[1];
    println!("Scanning directory: {}", input_dir);

    match scan_directory(input_dir) {
        Ok(report) => {
            println!("Scan Summary:");
            println!("  Root: {}", report.summary.root.display());
            println!("  Files supported: {}", report.summary.files_supported);
            println!("  Files ignored: {}", report.summary.files_ignored);
            println!("  Total size (bytes): {}", report.summary.total_size_bytes);

            if report.files.is_empty() {
                println!("No supported files found.");
                return;
            }

            println!("\nGenerating preview for up to 5 files...");
            let options = PreviewOptions {
                max_chars_per_file: 200,
                include_paths: true,
                max_files: Some(5),
            };

            match preview_files(&report.files, &options, None) {
                Ok(preview) => {
                    println!("\n{}", preview.combined_text);
                }
                Err(e) => {
                    eprintln!("Error generating preview: {:?}", e);
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
