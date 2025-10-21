use corpuswright_core::scan_directory;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixtures_dir = Path::new(manifest_dir).join("tests").join("fixtures");

    let scan_report = scan_directory(&fixtures_dir).expect("Failed to scan");
    println!("Total scanned files: {}", scan_report.files.len());
    for (i, f) in scan_report.files.iter().enumerate() {
        println!("File {}: {}", i, f.source_path.display());
    }

    Ok(())
}
