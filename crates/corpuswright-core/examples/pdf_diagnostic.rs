use corpuswright_core::pdf::{PdfExtractionOptions, extract_pdf};
use corpuswright_core::pdf_ocr;
use corpuswright_core::{PdfOcrQuality, PdfTextSource};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let pdf_path = args.first().map(PathBuf::from).ok_or_else(|| {
        anyhow::anyhow!(
            "usage: cargo run -p corpuswright-core --example pdf_diagnostic -- <pdf> [embedded|ocr-rescue|force-ocr] [find phrase] [max chars]"
        )
    })?;
    let mode = args
        .get(1)
        .and_then(|value| value.to_str())
        .unwrap_or("ocr-rescue");
    let find_phrase = args.get(2).and_then(|value| value.to_str());
    let max_chars = args
        .get(3)
        .and_then(|value| value.to_str())
        .map(str::parse::<usize>)
        .transpose()?;
    let mut options = PdfExtractionOptions::raw_default();
    match mode {
        "embedded" => {
            options.text_source = PdfTextSource::EmbeddedText;
            options.ocr_quality = PdfOcrQuality::Balanced;
        }
        "ocr-rescue" => {
            options.text_source = PdfTextSource::Ocr;
            options.ocr_quality = PdfOcrQuality::Balanced;
        }
        "force-ocr" => {
            options.text_source = PdfTextSource::ForceOcr;
            options.ocr_quality = PdfOcrQuality::HighQuality;
        }
        other => {
            return Err(anyhow::anyhow!(
                "unknown PDF diagnostic mode: {other}. Use embedded, ocr-rescue, or force-ocr."
            ));
        }
    }

    println!("PDF: {}", pdf_path.display());
    println!(
        "Mode: {:?}, OCR quality: {:?}",
        options.text_source, options.ocr_quality
    );
    println!(
        "Max chars: {}",
        max_chars
            .map(|limit| limit.to_string())
            .unwrap_or_else(|| "(none)".to_string())
    );
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
    match extract_pdf(&bytes, max_chars, options) {
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
            if let Some(find_phrase) = find_phrase {
                let found = extracted
                    .text
                    .to_lowercase()
                    .contains(&find_phrase.to_lowercase());
                println!("Find phrase {find_phrase:?}: {found}");
            }
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
