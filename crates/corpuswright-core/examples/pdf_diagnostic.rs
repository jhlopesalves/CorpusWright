use corpuswright_core::pdf::{PdfExtractionOptions, extract_pdf};
use corpuswright_core::pdf_ocr;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let pdf_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "usage: cargo run -p corpuswright-core --example pdf_diagnostic -- <pdf>"
            )
        })?;

    println!("PDF: {}", pdf_path.display());
    println!(
        "Configured OCR resource dir: {}",
        pdf_ocr::configured_ocr_resource_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(none)".to_string())
    );

    println!("OCR resource candidates:");
    for candidate in pdf_ocr::ocr_resource_candidates() {
        println!(
            "  {}{}",
            candidate.display(),
            if candidate.is_dir() { " [dir]" } else { "" }
        );
    }

    println!("PDFium library candidates:");
    for candidate in pdf_ocr::pdfium_library_candidates() {
        println!(
            "  {}{}",
            candidate.display(),
            if candidate.is_file() { " [file]" } else { "" }
        );
    }

    match pdf_ocr::init_pdfium() {
        Ok(_) => {
            let load_path = pdf_ocr::first_existing_pdfium_library()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "(system library or already initialized)".to_string());
            println!("PDFium init: ok");
            println!("PDFium load path: {load_path}");
        }
        Err(error) => {
            println!("PDFium init: failed");
            println!("PDFium unavailable: {error}");
        }
    }

    let bytes = std::fs::read(&pdf_path)?;
    match extract_pdf(
        &bytes,
        None,
        PdfExtractionOptions {
            use_ocr: true,
            ..PdfExtractionOptions::raw_default()
        },
    ) {
        Ok(extracted) => {
            let backend = extracted
                .warnings
                .iter()
                .find(|warning| warning.contains("PDF backend:"))
                .map(String::as_str)
                .unwrap_or("PDF backend: unknown");

            println!("{backend}");
            println!("Pages: {}", extracted.page_count);
            println!("Output chars: {}", extracted.text.chars().count());
            println!("Warnings:");
            for warning in &extracted.warnings {
                println!("  - {warning}");
            }
            println!("First 500 chars:");
            let preview: String = extracted.text.chars().take(500).collect();
            println!("{preview}");
        }
        Err(error) => {
            println!("Extraction failed: {error}");
        }
    }

    Ok(())
}
