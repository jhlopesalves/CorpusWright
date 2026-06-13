use corpuswright_core::pdf::{PdfExtractionOptions, extract_pdf, extract_pdf_page_range};
use corpuswright_core::{PdfOcrQuality, PdfTextSource, pdf_ocr, pdf_quality};
use lopdf::Document;
use pdfium_render::prelude::*;
use std::ffi::OsString;
use std::path::PathBuf;
use unicode_normalization::UnicodeNormalization;

const DEFAULT_PHRASE: &str = "frequency is not that important";
const DEFAULT_FRAGMENTS: [&str; 4] = [
    "frequency",
    "not that important",
    "that important",
    "important",
];
const PREVIEW_CHARS: usize = 180;
const WINDOW_CHARS: usize = 80;
const CODEPOINT_WINDOW_CHARS: usize = 24;

#[derive(Debug, Clone)]
enum DiagnosticCommand {
    ObjectScan,
    OcrRange { start_page: usize, end_page: usize },
    Scan,
    PageNumber(usize),
    PageIndex(usize),
    Range { start_page: usize, end_page: usize },
}

#[derive(Debug, Clone)]
struct Cli {
    command: DiagnosticCommand,
    pdf_path: PathBuf,
    phrase: String,
}

#[derive(Debug, Clone)]
struct NormalizedText {
    text: String,
    source_char_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct MatchHit {
    char_index: usize,
    window: String,
    codepoints: String,
}

#[derive(Debug, Clone)]
struct TermMatches {
    term: String,
    exact: Vec<MatchHit>,
    normalized: Vec<MatchHit>,
}

#[derive(Debug, Clone)]
struct PageReport {
    page_index: usize,
    page_number: usize,
    label: Option<String>,
    text: String,
    char_count: usize,
    quality: pdf_quality::ExtractionQuality,
    phrase: TermMatches,
    fragments: Vec<TermMatches>,
}

#[derive(Debug, Clone)]
struct PathBReport {
    text: String,
    page_texts: Vec<String>,
    warnings: Vec<String>,
    page_count: usize,
    phrase: TermMatches,
    fragments: Vec<TermMatches>,
}

#[derive(Debug, Clone, Default)]
struct PdfPageObjectSummary {
    page_index: usize,
    page_number: usize,
    embedded_chars: usize,
    text_objects: usize,
    image_objects: usize,
    path_objects: usize,
    shading_objects: usize,
    form_objects: usize,
    unsupported_objects: usize,
}

fn main() -> anyhow::Result<()> {
    let cli = parse_cli()?;
    match cli.command {
        DiagnosticCommand::ObjectScan => run_object_scan(cli),
        DiagnosticCommand::OcrRange { .. } => run_ocr_range(cli),
        _ => run_embedded_diagnostic(cli),
    }
}

fn parse_cli() -> anyhow::Result<Cli> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let usage = "usage:
  cargo run -p corpuswright-core --example pdf_diagnostic -- object-scan <pdf>
  cargo run -p corpuswright-core --example pdf_diagnostic -- ocr-range <pdf> <start 1-based page> <end 1-based page> [phrase]
  cargo run -p corpuswright-core --example pdf_diagnostic -- embedded-scan <pdf> [phrase]
  cargo run -p corpuswright-core --example pdf_diagnostic -- embedded-page <pdf> <1-based page number> [phrase]
  cargo run -p corpuswright-core --example pdf_diagnostic -- embedded-index <pdf> <0-based page index> [phrase]
  cargo run -p corpuswright-core --example pdf_diagnostic -- embedded-range <pdf> <start 1-based page> <end 1-based page> [phrase]";

    let command = args
        .first()
        .and_then(|arg| arg.to_str())
        .ok_or_else(|| anyhow::anyhow!(usage))?;

    match command {
        "object-scan" => {
            let pdf_path = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!(usage))?;
            Ok(Cli {
                command: DiagnosticCommand::ObjectScan,
                pdf_path,
                phrase: DEFAULT_PHRASE.to_string(),
            })
        }
        "ocr-range" => {
            let pdf_path = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!(usage))?;
            let start_page = parse_usize_arg(args.get(2), "start page")?;
            let end_page = parse_usize_arg(args.get(3), "end page")?;
            let phrase = phrase_arg(args.get(4));
            Ok(Cli {
                command: DiagnosticCommand::OcrRange {
                    start_page,
                    end_page,
                },
                pdf_path,
                phrase,
            })
        }
        "embedded-scan" => {
            let pdf_path = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!(usage))?;
            let phrase = phrase_arg(args.get(2));
            Ok(Cli {
                command: DiagnosticCommand::Scan,
                pdf_path,
                phrase,
            })
        }
        "embedded-page" => {
            let pdf_path = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!(usage))?;
            let page_number = parse_usize_arg(args.get(2), "page number")?;
            let phrase = phrase_arg(args.get(3));
            Ok(Cli {
                command: DiagnosticCommand::PageNumber(page_number),
                pdf_path,
                phrase,
            })
        }
        "embedded-index" => {
            let pdf_path = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!(usage))?;
            let page_index = parse_usize_arg(args.get(2), "page index")?;
            let phrase = phrase_arg(args.get(3));
            Ok(Cli {
                command: DiagnosticCommand::PageIndex(page_index),
                pdf_path,
                phrase,
            })
        }
        "embedded-range" => {
            let pdf_path = args
                .get(1)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!(usage))?;
            let start_page = parse_usize_arg(args.get(2), "start page")?;
            let end_page = parse_usize_arg(args.get(3), "end page")?;
            let phrase = phrase_arg(args.get(4));
            Ok(Cli {
                command: DiagnosticCommand::Range {
                    start_page,
                    end_page,
                },
                pdf_path,
                phrase,
            })
        }
        _ => Err(anyhow::anyhow!(usage)),
    }
}

fn run_ocr_range(cli: Cli) -> anyhow::Result<()> {
    if !cli.pdf_path.is_file() {
        return Err(anyhow::anyhow!("PDF not found: {}", cli.pdf_path.display()));
    }

    let DiagnosticCommand::OcrRange {
        start_page,
        end_page,
    } = cli.command
    else {
        return Err(anyhow::anyhow!("ocr-range command expected"));
    };

    if start_page == 0 || end_page < start_page {
        return Err(anyhow::anyhow!(
            "page range {start_page}..={end_page} is invalid"
        ));
    }

    let bytes = std::fs::read(&cli.pdf_path)?;
    let result = extract_pdf_page_range(
        &bytes,
        start_page - 1,
        end_page - start_page + 1,
        PdfExtractionOptions {
            text_source: PdfTextSource::ForceOcr,
            ocr_quality: PdfOcrQuality::HighQuality,
            ..PdfExtractionOptions::raw_default()
        },
        None,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let mut combined_text = String::new();
    println!("OCR page-range diagnostic");
    println!("PDF path: {}", cli.pdf_path.display());
    println!("Requested pages: {start_page}..={end_page}");
    println!("PDF page count: {}", result.page_count);
    println!("Returned page count: {}", result.pages.len());
    println!("Phrase: {:?}", cli.phrase);
    for warning in &result.warnings {
        println!("Global warning: {warning}");
    }
    println!();

    for page in &result.pages {
        if !combined_text.is_empty() {
            combined_text.push('\n');
            combined_text.push('\n');
        }
        combined_text.push_str(&page.text);

        let exact_found = page.text.contains(&cli.phrase);
        let normalized_found = !normalized_hits(&page.text, &cli.phrase).is_empty();
        println!("Page {}", page.page_number);
        println!("  chars: {}", page.char_count);
        println!("  method: {:?}", page.method);
        println!("  render clamped: {}", yes_no(page.render_clamped));
        println!("  exact phrase match: {}", yes_no(exact_found));
        println!("  normalised phrase match: {}", yes_no(normalized_found));
        if let Some(error) = &page.error {
            println!("  error: {error}");
        }
        for warning in &page.warnings {
            println!("  warning: {warning}");
        }
        println!(
            "  first {} chars: {}",
            PREVIEW_CHARS,
            visible_text(&take_chars(&page.text, PREVIEW_CHARS))
        );
    }
    println!();
    println!(
        "Range exact phrase match: {}",
        yes_no(combined_text.contains(&cli.phrase))
    );
    println!(
        "Range normalised phrase match: {}",
        yes_no(!normalized_hits(&combined_text, &cli.phrase).is_empty())
    );

    Ok(())
}

fn run_object_scan(cli: Cli) -> anyhow::Result<()> {
    if !cli.pdf_path.is_file() {
        return Err(anyhow::anyhow!("PDF not found: {}", cli.pdf_path.display()));
    }

    let metadata = std::fs::metadata(&cli.pdf_path)?;
    let bytes = std::fs::read(&cli.pdf_path)?;
    let lopdf_doc = Document::load_mem(&bytes)?;
    let lopdf_version = lopdf_doc.version.clone();
    let encrypted = lopdf_doc.is_encrypted();

    let pdfium = pdf_ocr::init_pdfium()?;
    let document = {
        let _lock = corpuswright_core::pdf::PDFIUM_LOCK.lock().unwrap();
        pdfium.load_pdf_from_byte_slice(&bytes, None)?
    };
    let document_version = format!("{:?}", document.version());
    let page_count = {
        let _lock = corpuswright_core::pdf::PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    let page_summaries = object_summaries_from_pdfium(&document, page_count)?;
    let total_embedded_chars: usize = page_summaries.iter().map(|page| page.embedded_chars).sum();
    let zero_text_pages = page_summaries
        .iter()
        .filter(|page| page.embedded_chars == 0)
        .count();
    let total_text_objects: usize = page_summaries.iter().map(|page| page.text_objects).sum();
    let total_image_objects: usize = page_summaries.iter().map(|page| page.image_objects).sum();
    let pages_with_images = page_summaries
        .iter()
        .filter(|page| page.image_objects > 0)
        .count();
    let image_only_pages = page_summaries
        .iter()
        .filter(|page| page.embedded_chars == 0 && page.text_objects == 0 && page.image_objects > 0)
        .count();
    let pages_with_text_objects = page_summaries
        .iter()
        .filter(|page| page.text_objects > 0)
        .count();

    println!("PDF object diagnostic");
    println!("PDF path: {}", cli.pdf_path.display());
    println!("File size: {} bytes", metadata.len());
    println!("Page count: {page_count}");
    println!("PDF version: PDFium {document_version}; lopdf header {lopdf_version}");
    println!(
        "Encrypted/password-protected: {}",
        if encrypted { "yes" } else { "no" }
    );
    println!("Permission flags: not exposed by the wrappers used here");
    println!("pdfium-render crate: 0.9.1");
    println!("PDFium API version/build: not exposed through the public wrapper used here");
    println!(
        "PDFium load path: {}",
        pdf_ocr::first_existing_pdfium_library()
            .map(|path| format!("dynamic candidate {}", path.display()))
            .unwrap_or_else(|| "system library or already initialised".to_string())
    );
    println!();
    println!("Text-vs-image aggregate");
    println!("  Total embedded text characters: {total_embedded_chars}");
    println!("  Pages with zero embedded text: {zero_text_pages}");
    println!("  Pages with text objects: {pages_with_text_objects}");
    println!("  Total text objects: {total_text_objects}");
    println!("  Total image objects: {total_image_objects}");
    println!("  Pages with image objects: {pages_with_images}");
    println!("  Pages that appear image-only: {image_only_pages}");
    println!();
    println!("Representative page summaries");
    print_object_page_summary("first page", page_summaries.first());
    print_object_page_summary(
        "page 32 or nearby",
        representative_page(&page_summaries, 31),
    );
    print_object_page_summary(
        "first page with text objects",
        page_summaries.iter().find(|page| page.text_objects > 0),
    );
    println!();
    println!("Gate condition");
    let confirms_image_only = total_embedded_chars == 0
        && total_text_objects == 0
        && page_count > 0
        && pages_with_images >= page_count.saturating_sub(1);
    println!(
        "  Image-only scanned structure confirmed: {}",
        yes_no(confirms_image_only)
    );

    Ok(())
}

fn object_summaries_from_pdfium(
    document: &PdfDocument<'_>,
    page_count: usize,
) -> anyhow::Result<Vec<PdfPageObjectSummary>> {
    let mut summaries = Vec::with_capacity(page_count);

    for page_index in 0..page_count {
        let summary = {
            let _lock = corpuswright_core::pdf::PDFIUM_LOCK.lock().unwrap();
            let page = document.pages().get(page_index as i32)?;
            let embedded_chars = page.text()?.all().chars().count();
            let mut summary = PdfPageObjectSummary {
                page_index,
                page_number: page_index + 1,
                embedded_chars,
                ..PdfPageObjectSummary::default()
            };

            for object in page.objects().iter() {
                match object.object_type() {
                    PdfPageObjectType::Text => summary.text_objects += 1,
                    PdfPageObjectType::Image => summary.image_objects += 1,
                    PdfPageObjectType::Path => summary.path_objects += 1,
                    PdfPageObjectType::Shading => summary.shading_objects += 1,
                    PdfPageObjectType::XObjectForm => summary.form_objects += 1,
                    PdfPageObjectType::Unsupported => summary.unsupported_objects += 1,
                }
            }

            summary
        };
        summaries.push(summary);
    }

    Ok(summaries)
}

fn representative_page(
    summaries: &[PdfPageObjectSummary],
    target_index: usize,
) -> Option<&PdfPageObjectSummary> {
    if summaries.is_empty() {
        None
    } else {
        summaries.get(target_index.min(summaries.len() - 1))
    }
}

fn print_object_page_summary(label: &str, summary: Option<&PdfPageObjectSummary>) {
    println!("  {label}:");
    if let Some(summary) = summary {
        println!(
            "    page index {} / physical {}",
            summary.page_index, summary.page_number
        );
        println!("    embedded characters: {}", summary.embedded_chars);
        println!("    text objects: {}", summary.text_objects);
        println!("    image objects: {}", summary.image_objects);
        println!("    path objects: {}", summary.path_objects);
        println!("    shading objects: {}", summary.shading_objects);
        println!("    form objects: {}", summary.form_objects);
        println!("    unsupported objects: {}", summary.unsupported_objects);
        println!(
            "    appears image-only: {}",
            yes_no(
                summary.embedded_chars == 0
                    && summary.text_objects == 0
                    && summary.image_objects > 0
            )
        );
    } else {
        println!("    unavailable");
    }
}

fn phrase_arg(value: Option<&OsString>) -> String {
    value
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_else(|| DEFAULT_PHRASE.to_string())
}

fn parse_usize_arg(value: Option<&OsString>, name: &str) -> anyhow::Result<usize> {
    value
        .and_then(|arg| arg.to_str())
        .ok_or_else(|| anyhow::anyhow!("missing {name}"))?
        .parse::<usize>()
        .map_err(|error| anyhow::anyhow!("invalid {name}: {error}"))
}

fn run_embedded_diagnostic(cli: Cli) -> anyhow::Result<()> {
    if !cli.pdf_path.is_file() {
        return Err(anyhow::anyhow!("PDF not found: {}", cli.pdf_path.display()));
    }

    let metadata = std::fs::metadata(&cli.pdf_path)?;
    let bytes = std::fs::read(&cli.pdf_path)?;
    let fragments = search_fragments(&cli.phrase);

    let pdfium_init = pdf_ocr::init_pdfium();
    let pdfium = match pdfium_init {
        Ok(pdfium) => pdfium,
        Err(error) => {
            print_pdf_header(&cli, &metadata, None, Some(&error.to_string()));
            return Err(error);
        }
    };

    let document = {
        let _lock = corpuswright_core::pdf::PDFIUM_LOCK.lock().unwrap();
        pdfium.load_pdf_from_byte_slice(&bytes, None)?
    };
    let document_version = format!("{:?}", document.version());
    let page_count = {
        let _lock = corpuswright_core::pdf::PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    let selected_pages = selected_page_indices(&cli.command, page_count)?;
    let all_pages = direct_pdfium_pages(&document, page_count, &cli.phrase, &fragments)?;
    let path_b = corpuswright_embedded_path(&bytes, &all_pages, &cli.phrase, &fragments)?;

    print_pdf_header(&cli, &metadata, Some(&document_version), None);
    print_page_selection(&cli.command, &selected_pages);
    print_normalization_note();
    print_path_summaries(&all_pages, &path_b);
    print_phrase_summaries(&cli.phrase, &fragments, &all_pages, &path_b);
    print_selected_page_reports(&all_pages, &selected_pages);
    print_path_b_warnings(&path_b);
    print_interpretation(&all_pages, &path_b);

    Ok(())
}

fn print_pdf_header(
    cli: &Cli,
    metadata: &std::fs::Metadata,
    document_version: Option<&str>,
    pdfium_error: Option<&str>,
) {
    println!("Embedded PDF diagnostic");
    println!("PDF path: {}", cli.pdf_path.display());
    println!("File size: {} bytes", metadata.len());
    println!("Command: {:?}", cli.command);
    println!("Phrase: {:?}", cli.phrase);
    println!("pdfium-render crate: 0.9.1");
    println!("PDFium API version/build: not exposed through the public wrapper used here");
    println!(
        "PDFium load path: {}",
        pdf_ocr::first_existing_pdfium_library()
            .map(|path| format!("dynamic candidate {}", path.display()))
            .unwrap_or_else(|| "system library or already initialised".to_string())
    );
    println!(
        "PDF document version: {}",
        document_version.unwrap_or("(unavailable)")
    );
    if let Some(error) = pdfium_error {
        println!("PDFium init: failed: {error}");
    } else {
        println!("PDFium init: ok");
    }
    println!("Page labels: reported from PdfPage::label() when present");
    println!();
}

fn print_page_selection(command: &DiagnosticCommand, selected_pages: &[usize]) {
    let selected = selected_pages
        .iter()
        .map(|page_index| format!("{} (physical {})", page_index, page_index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    println!("Selected page detail mode: {:?}", command);
    println!("Selected page details: {selected}");
    println!("Full-document aggregate: always computed");
    println!();
}

fn print_normalization_note() {
    println!("Normalised matching:");
    println!("  - applies Unicode NFKC");
    println!("  - expands common fi/fl/ff/ffi/ffl ligatures");
    println!("  - removes soft hyphen and zero-width format characters");
    println!("  - folds non-breaking and Unicode whitespace to spaces");
    println!("  - collapses whitespace runs, trims, and compares case-insensitively");
    println!(
        "  - removes a hyphen only when alphabetic text is split as letter-hyphen-whitespace-letter"
    );
    println!();
}

fn selected_page_indices(
    command: &DiagnosticCommand,
    page_count: usize,
) -> anyhow::Result<Vec<usize>> {
    match *command {
        DiagnosticCommand::ObjectScan | DiagnosticCommand::OcrRange { .. } => Ok(Vec::new()),
        DiagnosticCommand::Scan => Ok((0..page_count).collect()),
        DiagnosticCommand::PageNumber(page_number) => {
            if page_number == 0 || page_number > page_count {
                return Err(anyhow::anyhow!(
                    "page number {page_number} is outside 1..={page_count}"
                ));
            }
            Ok(vec![page_number - 1])
        }
        DiagnosticCommand::PageIndex(page_index) => {
            if page_index >= page_count {
                return Err(anyhow::anyhow!(
                    "page index {page_index} is outside 0..{}",
                    page_count.saturating_sub(1)
                ));
            }
            Ok(vec![page_index])
        }
        DiagnosticCommand::Range {
            start_page,
            end_page,
        } => {
            if start_page == 0 || end_page == 0 || start_page > end_page || end_page > page_count {
                return Err(anyhow::anyhow!(
                    "page range {start_page}..={end_page} is outside 1..={page_count}"
                ));
            }
            Ok((start_page - 1..end_page).collect())
        }
    }
}

fn direct_pdfium_pages(
    document: &PdfDocument<'_>,
    page_count: usize,
    phrase: &str,
    fragments: &[String],
) -> anyhow::Result<Vec<PageReport>> {
    let mut pages = Vec::with_capacity(page_count);

    for page_index in 0..page_count {
        let (label, text) = {
            let _lock = corpuswright_core::pdf::PDFIUM_LOCK.lock().unwrap();
            let page = document.pages().get(page_index as i32)?;
            let label = page.label().map(ToOwned::to_owned);
            let text_page = page.text()?;
            (label, text_page.all())
        };

        let char_count = text.chars().count();
        let (quality, _) = pdf_quality::evaluate(&text);
        pages.push(PageReport {
            page_index,
            page_number: page_index + 1,
            label,
            phrase: analyse_term(&text, phrase),
            fragments: fragments
                .iter()
                .map(|fragment| analyse_term(&text, fragment))
                .collect(),
            text,
            char_count,
            quality,
        });
    }

    Ok(pages)
}

fn corpuswright_embedded_path(
    bytes: &[u8],
    direct_pages: &[PageReport],
    phrase: &str,
    fragments: &[String],
) -> anyhow::Result<PathBReport> {
    let extracted = extract_pdf(bytes, None, PdfExtractionOptions::raw_default())
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let page_texts = direct_pages
        .iter()
        .map(|page| corpuswright_raw_page_text(&page.text))
        .collect::<Vec<_>>();

    Ok(PathBReport {
        phrase: analyse_term(&extracted.text, phrase),
        fragments: fragments
            .iter()
            .map(|fragment| analyse_term(&extracted.text, fragment))
            .collect(),
        text: extracted.text,
        page_texts,
        warnings: extracted.warnings,
        page_count: extracted.page_count,
    })
}

fn corpuswright_raw_page_text(text: &str) -> String {
    text.lines()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn reconstruct_path_b_text(page_texts: &[String]) -> String {
    page_texts
        .iter()
        .filter(|text| !text.trim().is_empty())
        .map(|text| text.trim())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn print_path_summaries(path_a: &[PageReport], path_b: &PathBReport) {
    let counts = path_a
        .iter()
        .map(|page| page.char_count)
        .collect::<Vec<_>>();
    let path_a_total: usize = counts.iter().sum();
    let path_a_zero = counts.iter().filter(|count| **count == 0).count();
    let path_a_nonzero = counts.len().saturating_sub(path_a_zero);
    let (min, median, max) = min_median_max(&counts).unwrap_or((0, 0, 0));
    let path_b_counts = path_b
        .page_texts
        .iter()
        .map(|text| text.chars().count())
        .collect::<Vec<_>>();
    let path_b_zero = path_b_counts.iter().filter(|count| **count == 0).count();
    let path_b_nonzero = path_b_counts.len().saturating_sub(path_b_zero);
    let reconstructed_b = reconstruct_path_b_text(&path_b.page_texts);
    let path_b_total = path_b.text.chars().count();

    println!("Path A: clean-room direct PDFium page.text().all()");
    println!("  Page count: {}", path_a.len());
    println!("  Total embedded characters: {path_a_total}");
    println!("  Zero-text pages: {path_a_zero}");
    println!("  Non-zero-text pages: {path_a_nonzero}");
    println!("  Min/median/max chars per page: {min}/{median}/{max}");
    print_quality_summary(path_a);
    println!();

    println!("Path B: CorpusWright extract_pdf(..., PdfExtractionOptions::raw_default())");
    println!("  Strategy: PdfiumFlat, PdfTextSource::EmbeddedText, no OCR, no cleanup flags");
    println!(
        "  Path B joins trimmed non-empty raw pages with blank lines and runs quality warnings"
    );
    println!("  Page count: {}", path_b.page_count);
    println!("  Total embedded characters: {path_b_total}");
    println!("  Zero-text pages, reconstructed from raw-default page trimming: {path_b_zero}");
    println!(
        "  Non-zero-text pages, reconstructed from raw-default page trimming: {path_b_nonzero}"
    );
    println!(
        "  Reconstructed Path B text equals extract_pdf output: {}",
        yes_no(reconstructed_b == path_b.text)
    );
    println!(
        "  Path B total differs from Path A raw total: {}",
        yes_no(path_b_total != path_a_total)
    );
    println!(
        "  Path B text differs from Path A raw joined pages: {}",
        yes_no(path_b.text != path_a_raw_joined(path_a))
    );
    println!();
}

fn print_quality_summary(path_a: &[PageReport]) {
    let mut good = 0;
    let mut suspicious = 0;
    let mut poor = 0;
    let mut empty = 0;
    let mut notable = Vec::new();

    for page in path_a {
        match page.quality {
            pdf_quality::ExtractionQuality::Good => good += 1,
            pdf_quality::ExtractionQuality::Suspicious => {
                suspicious += 1;
                notable.push(page.page_index);
            }
            pdf_quality::ExtractionQuality::Poor => {
                poor += 1;
                notable.push(page.page_index);
            }
            pdf_quality::ExtractionQuality::Empty => {
                empty += 1;
                notable.push(page.page_index);
            }
        }
    }

    println!("  Quality counts: good={good}, suspicious={suspicious}, poor={poor}, empty={empty}");
    println!(
        "  Suspicious/poor/empty page indices: {}",
        format_page_list(notable.into_iter().take(12))
    );
}

fn path_a_raw_joined(path_a: &[PageReport]) -> String {
    path_a
        .iter()
        .map(|page| page.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn min_median_max(values: &[usize]) -> Option<(usize, usize, usize)> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Some((
        *sorted.first().unwrap(),
        sorted[sorted.len() / 2],
        *sorted.last().unwrap(),
    ))
}

fn print_phrase_summaries(
    phrase: &str,
    fragments: &[String],
    path_a: &[PageReport],
    path_b: &PathBReport,
) {
    println!("Phrase summary");
    println!("  Phrase: {:?}", phrase);
    println!(
        "  Path A exact phrase pages: {}",
        pages_with(path_a, |page| !page.phrase.exact.is_empty())
    );
    println!(
        "  Path A normalised phrase pages: {}",
        pages_with(path_a, |page| !page.phrase.normalized.is_empty())
    );
    println!(
        "  Path B exact phrase match: {}",
        yes_no(!path_b.phrase.exact.is_empty())
    );
    println!(
        "  Path B normalised phrase match: {}",
        yes_no(!path_b.phrase.normalized.is_empty())
    );
    println!("  Fragments:");
    for fragment in fragments {
        let path_a_exact = pages_with(path_a, |page| {
            page.fragments
                .iter()
                .any(|matches| matches.term == *fragment && !matches.exact.is_empty())
        });
        let path_a_normalized = pages_with(path_a, |page| {
            page.fragments
                .iter()
                .any(|matches| matches.term == *fragment && !matches.normalized.is_empty())
        });
        let path_b_fragment = path_b
            .fragments
            .iter()
            .find(|matches| matches.term == *fragment);
        println!(
            "    {:?}: Path A exact pages [{}], Path A normalised pages [{}], Path B exact={}, Path B normalised={}",
            fragment,
            path_a_exact,
            path_a_normalized,
            yes_no(path_b_fragment.is_some_and(|matches| !matches.exact.is_empty())),
            yes_no(path_b_fragment.is_some_and(|matches| !matches.normalized.is_empty()))
        );
    }
    println!();
}

fn pages_with<F>(pages: &[PageReport], predicate: F) -> String
where
    F: Fn(&PageReport) -> bool,
{
    format_page_list(
        pages
            .iter()
            .filter(|page| predicate(page))
            .map(|page| page.page_index),
    )
}

fn format_page_list<I>(page_indices: I) -> String
where
    I: IntoIterator<Item = usize>,
{
    let pages = page_indices
        .into_iter()
        .map(|page_index| format!("{} / physical {}", page_index, page_index + 1))
        .collect::<Vec<_>>();
    if pages.is_empty() {
        "none".to_string()
    } else {
        pages.join(", ")
    }
}

fn print_selected_page_reports(path_a: &[PageReport], selected_pages: &[usize]) {
    println!("Selected Path A page details");
    for page_index in selected_pages {
        let Some(page) = path_a.get(*page_index) else {
            continue;
        };
        println!(
            "Page index {} / physical {} / label {}",
            page.page_index,
            page.page_number,
            page.label.as_deref().unwrap_or("(none)")
        );
        println!("  Embedded characters: {}", page.char_count);
        println!("  Quality: {:?}", page.quality);
        println!(
            "  Exact phrase match: {}",
            yes_no(!page.phrase.exact.is_empty())
        );
        println!(
            "  Normalised phrase match: {}",
            yes_no(!page.phrase.normalized.is_empty())
        );
        println!(
            "  First {} extracted chars: {}",
            PREVIEW_CHARS,
            visible_text(&take_chars(&page.text, PREVIEW_CHARS))
        );
        print_term_windows("phrase", &page.phrase);
        for fragment in &page.fragments {
            if !fragment.exact.is_empty() || !fragment.normalized.is_empty() {
                print_term_windows("fragment", fragment);
            }
        }
        if page.phrase.exact.is_empty()
            && page.phrase.normalized.is_empty()
            && page
                .fragments
                .iter()
                .any(|fragment| !fragment.exact.is_empty() || !fragment.normalized.is_empty())
        {
            println!("  Partial-match codepoint diagnostic:");
            if let Some(fragment) = page
                .fragments
                .iter()
                .find(|fragment| !fragment.exact.is_empty() || !fragment.normalized.is_empty())
            {
                let hit = fragment
                    .exact
                    .first()
                    .or_else(|| fragment.normalized.first())
                    .unwrap();
                println!("    {:?}: {}", fragment.term, hit.codepoints);
            }
        }
    }
    println!();
}

fn print_term_windows(label: &str, matches: &TermMatches) {
    for hit in matches.exact.iter().take(2) {
        println!(
            "  {label} exact {:?} at char {}: {}",
            matches.term, hit.char_index, hit.window
        );
    }
    for hit in matches.normalized.iter().take(2) {
        println!(
            "  {label} normalised {:?} near char {}: {}",
            matches.term, hit.char_index, hit.window
        );
    }
}

fn print_path_b_warnings(path_b: &PathBReport) {
    println!("Path B warnings");
    if path_b.warnings.is_empty() {
        println!("  (none)");
    } else {
        for warning in &path_b.warnings {
            println!("  - {warning}");
        }
    }
    println!();
}

fn print_interpretation(path_a: &[PageReport], path_b: &PathBReport) {
    let path_a_phrase = path_a
        .iter()
        .any(|page| !page.phrase.exact.is_empty() || !page.phrase.normalized.is_empty());
    let path_b_phrase = !path_b.phrase.exact.is_empty() || !path_b.phrase.normalized.is_empty();
    let path_a_total: usize = path_a.iter().map(|page| page.char_count).sum();
    let path_b_total = path_b.text.chars().count();

    println!("Diagnostic interpretation");
    println!(
        "  Previous zero embedded characters diagnosis confirmed: {}",
        yes_no(path_a_total == 0 && path_b_total == 0)
    );
    println!(
        "  Path A and Path B agree on full phrase visibility: {}",
        yes_no(path_a_phrase == path_b_phrase)
    );
    println!(
        "  Path A and Path B both have embedded text: {}",
        yes_no(path_a_total > 0 && path_b_total > 0)
    );
    if path_a_phrase && !path_b_phrase {
        println!(
            "  Likely diagnosis: CorpusWright extraction/routing/cache/cleaning/strategy layer."
        );
    } else if path_a_phrase && path_b_phrase {
        println!(
            "  Likely diagnosis: embedded extraction sees the phrase; look next at frontend preview/search/config usage."
        );
    } else {
        println!(
            "  Likely diagnosis: neither embedded path sees the phrase; verify file identity and Chrome/PDFium assumptions."
        );
    }
}

fn analyse_term(text: &str, term: &str) -> TermMatches {
    TermMatches {
        term: term.to_string(),
        exact: exact_hits(text, term),
        normalized: normalized_hits(text, term),
    }
}

fn exact_hits(text: &str, term: &str) -> Vec<MatchHit> {
    if term.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    let mut search_start = 0;
    while let Some(relative) = text[search_start..].find(term) {
        let byte_index = search_start + relative;
        let char_index = text[..byte_index].chars().count();
        hits.push(match_hit(text, char_index, term.chars().count()));
        search_start = byte_index + term.len().max(1);
    }
    hits
}

fn normalized_hits(text: &str, term: &str) -> Vec<MatchHit> {
    let normalized_text = normalize_for_search(text);
    let normalized_term = normalize_for_search(term);
    if normalized_term.text.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    let mut search_start = 0;
    while let Some(relative) = normalized_text.text[search_start..].find(&normalized_term.text) {
        let byte_index = search_start + relative;
        let normalized_char_index = normalized_text.text[..byte_index].chars().count();
        if let Some(&source_char_index) = normalized_text
            .source_char_indices
            .get(normalized_char_index)
        {
            hits.push(match_hit(
                text,
                source_char_index,
                normalized_term.text.chars().count(),
            ));
        }
        search_start = byte_index + normalized_term.text.len().max(1);
    }
    hits
}

fn match_hit(text: &str, char_index: usize, term_len: usize) -> MatchHit {
    MatchHit {
        char_index,
        window: visible_text(&window_around(text, char_index, term_len, WINDOW_CHARS)),
        codepoints: codepoint_window(text, char_index),
    }
}

fn window_around(text: &str, char_index: usize, term_len: usize, context: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let start = char_index.saturating_sub(context);
    let end = (char_index + term_len + context).min(chars.len());
    chars[start..end].iter().collect()
}

fn codepoint_window(text: &str, char_index: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let start = char_index.saturating_sub(CODEPOINT_WINDOW_CHARS / 2);
    let end = (char_index + CODEPOINT_WINDOW_CHARS / 2).min(chars.len());
    chars[start..end]
        .iter()
        .map(|ch| format!("U+{:04X} '{}'", *ch as u32, visible_char(*ch)))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn normalize_for_search(input: &str) -> NormalizedText {
    let input_chars = input.chars().enumerate().collect::<Vec<_>>();
    let mut raw = Vec::<(char, usize)>::new();
    let mut i = 0;

    while i < input_chars.len() {
        let (source_index, ch) = input_chars[i];
        if ch == '\u{00AD}' || is_zero_width(ch) {
            i += 1;
            continue;
        }
        if is_line_hyphenation(&input_chars, i) {
            i += 1;
            while i < input_chars.len() && input_chars[i].1.is_whitespace() {
                i += 1;
            }
            continue;
        }

        for normalized in expand_char(ch).nfkc() {
            let folded = if normalized == '\u{00A0}' || normalized.is_whitespace() {
                ' '
            } else {
                normalized
            };
            for lower in folded.to_lowercase() {
                raw.push((lower, source_index));
            }
        }
        i += 1;
    }

    collapse_normalized_whitespace(raw)
}

fn expand_char(ch: char) -> String {
    match ch {
        '\u{FB01}' => "fi".to_string(),
        '\u{FB02}' => "fl".to_string(),
        '\u{FB00}' => "ff".to_string(),
        '\u{FB03}' => "ffi".to_string(),
        '\u{FB04}' => "ffl".to_string(),
        '\u{00A0}' => " ".to_string(),
        _ => ch.to_string(),
    }
}

fn collapse_normalized_whitespace(raw: Vec<(char, usize)>) -> NormalizedText {
    let mut text = String::new();
    let mut source_char_indices = Vec::new();
    let mut previous_space = false;

    for (mut ch, source_index) in raw {
        if ch == '\0' {
            continue;
        }
        if ch.is_whitespace() {
            ch = ' ';
        }
        if ch == ' ' {
            if text.is_empty() || previous_space {
                continue;
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
        text.push(ch);
        source_char_indices.push(source_index);
    }

    if text.ends_with(' ') {
        text.pop();
        source_char_indices.pop();
    }

    NormalizedText {
        text,
        source_char_indices,
    }
}

fn is_line_hyphenation(chars: &[(usize, char)], index: usize) -> bool {
    let ch = chars[index].1;
    if !matches!(ch, '-' | '\u{2010}' | '\u{2011}') {
        return false;
    }
    if index == 0 || index + 1 >= chars.len() || !chars[index + 1].1.is_whitespace() {
        return false;
    }

    let previous = chars[..index]
        .iter()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace() && !is_zero_width(*ch));
    let next = chars[index + 1..]
        .iter()
        .find(|(_, ch)| !ch.is_whitespace() && !is_zero_width(*ch));

    previous.is_some_and(|(_, ch)| ch.is_alphabetic())
        && next.is_some_and(|(_, ch)| ch.is_alphabetic())
}

fn is_zero_width(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{180E}'
    )
}

fn search_fragments(phrase: &str) -> Vec<String> {
    let mut fragments = Vec::new();
    for fragment in DEFAULT_FRAGMENTS {
        if fragment != phrase && !fragments.iter().any(|seen| seen == fragment) {
            fragments.push(fragment.to_string());
        }
    }
    fragments
}

fn visible_text(input: &str) -> String {
    let mut visible = String::new();
    for ch in input.chars() {
        match ch {
            '\n' => visible.push_str("\\n"),
            '\r' => visible.push_str("\\r"),
            '\t' => visible.push_str("\\t"),
            ch if ch.is_control() => visible.push_str(&format!("\\u{{{:04X}}}", ch as u32)),
            ch => visible.push(ch),
        }
    }
    visible
}

fn visible_char(ch: char) -> String {
    match ch {
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        ch if ch.is_control() => format!("\\u{{{:04X}}}", ch as u32),
        ch => ch.to_string(),
    }
}

fn take_chars(input: &str, limit: usize) -> String {
    input.chars().take(limit).collect()
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
