use crate::pdf::PDFIUM_LOCK;
use crate::pdf_quality::ExtractionQuality;
use lopdf::Document;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

const AUDIT_MAX_PAGES: usize = 3;
const AUDIT_MAX_CHARS: usize = 5_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfAuditQuality {
    Good,
    Suspicious,
    Poor,
    Empty,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfAuditSuggestedProfile {
    Standard,
    LayoutHeavy,
    OcrRescue,
    ForceOcr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PdfAuditResult {
    pub path: String,
    pub file_name: String,
    pub page_count: Option<usize>,
    pub sampled_page_count: usize,
    pub embedded_text_detected: bool,
    pub embedded_text_chars: usize,
    pub quality: PdfAuditQuality,
    pub pdfium_available: bool,
    pub ocr_model_resources_available: bool,
    pub ocr_full_usability_checked: bool,
    pub degraded_fallback_used: bool,
    pub suggested_profile: PdfAuditSuggestedProfile,
    pub warnings: Vec<String>,
}

struct PdfAuditEnvironment {
    pdfium: Result<&'static pdfium_render::prelude::Pdfium, String>,
    ocr_model_resources_available: bool,
}

struct PdfAuditSample {
    text: String,
    page_count: Option<usize>,
    sampled_page_count: usize,
    degraded_fallback_used: bool,
    warnings: Vec<String>,
}

pub fn audit_pdf_files(paths: Vec<PathBuf>) -> Vec<PdfAuditResult> {
    let environment = PdfAuditEnvironment {
        pdfium: crate::pdf_ocr::init_pdfium().map_err(|error| error.to_string()),
        ocr_model_resources_available: crate::pdf_ocr::ocr_model_resources_available(),
    };

    paths
        .iter()
        .map(|path| audit_pdf_file(path, &environment))
        .collect()
}

fn audit_pdf_file(path: &Path, environment: &PdfAuditEnvironment) -> PdfAuditResult {
    let path_display = path.display().to_string();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path_display.as_str())
        .to_string();
    let pdfium_available = environment.pdfium.is_ok();

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return failed_result(
                path_display,
                file_name,
                pdfium_available,
                environment.ocr_model_resources_available,
                format!("Could not read PDF for audit: {error}"),
            );
        }
    };

    let doc = match Document::load_mem(&bytes) {
        Ok(doc) => doc,
        Err(error) => {
            return failed_result(
                path_display,
                file_name,
                pdfium_available,
                environment.ocr_model_resources_available,
                format!("Could not parse PDF for audit: {error}"),
            );
        }
    };

    let sample = audit_pdf_bytes(&bytes, &doc, environment);
    let mut warnings = sample.warnings;
    let (quality, embedded_text_chars, embedded_text_detected) = classify_sample_text(&sample.text);
    let suggested_profile = suggested_profile_for_quality(&quality, embedded_text_chars);

    match quality {
        PdfAuditQuality::Empty => warnings.push(
            "This PDF appears to contain little or no embedded text. Force OCR is recommended."
                .to_string(),
        ),
        PdfAuditQuality::Poor => warnings.push(
            "Embedded text was found, but extraction quality looks very poor. Force OCR is recommended and preview should be inspected."
                .to_string(),
        ),
        PdfAuditQuality::Suspicious => warnings.push(
            "Embedded text was found, but extraction quality looks suspicious. Try OCR rescue or Layout-heavy PDF and inspect the preview."
                .to_string(),
        ),
        PdfAuditQuality::Good | PdfAuditQuality::Unknown => {}
    }

    if !environment.ocr_model_resources_available {
        warnings.push(
            "OCR model files are unavailable; OCR profiles may not run until the OCR resources are installed."
                .to_string(),
        );
    }

    if matches!(
        suggested_profile,
        PdfAuditSuggestedProfile::OcrRescue | PdfAuditSuggestedProfile::ForceOcr
    ) && environment.ocr_model_resources_available
    {
        warnings.push(
            "An OCR profile is suggested, but this audit only checks OCR model files and does not verify full OCR usability."
                .to_string(),
        );
    }

    PdfAuditResult {
        path: path_display,
        file_name,
        page_count: sample.page_count,
        sampled_page_count: sample.sampled_page_count,
        embedded_text_detected,
        embedded_text_chars,
        quality,
        pdfium_available,
        ocr_model_resources_available: environment.ocr_model_resources_available,
        ocr_full_usability_checked: false,
        degraded_fallback_used: sample.degraded_fallback_used,
        suggested_profile,
        warnings,
    }
}

fn audit_pdf_bytes(
    bytes: &[u8],
    doc: &Document,
    environment: &PdfAuditEnvironment,
) -> PdfAuditSample {
    let mut warnings = Vec::new();
    if doc.is_encrypted() {
        warnings.push(
            "PDF is encrypted or password protected. Text extraction may fail or produce garbage."
                .to_string(),
        );
    }

    match environment.pdfium {
        Ok(pdfium) => match sample_with_pdfium(pdfium, bytes) {
            Ok(mut sample) => {
                sample.warnings.splice(0..0, warnings);
                sample
            }
            Err(error) => {
                warnings.push(format!(
                    "PDFium could not audit this PDF; used degraded lopdf fallback. Results may be incomplete. ({error})"
                ));
                sample_with_lopdf_fallback(doc, warnings)
            }
        },
        Err(ref error) => {
            warnings.push(format!(
                "PDFium is unavailable; audit used degraded lopdf fallback. Results may be incomplete. ({error})"
            ));
            sample_with_lopdf_fallback(doc, warnings)
        }
    }
}

fn sample_with_pdfium(
    pdfium: &'static pdfium_render::prelude::Pdfium,
    bytes: &[u8],
) -> anyhow::Result<PdfAuditSample> {
    let document = {
        let _lock = PDFIUM_LOCK.lock().unwrap();
        pdfium.load_pdf_from_byte_slice(bytes, None)?
    };

    let page_count = {
        let _lock = PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };
    let pages_to_sample = page_count.min(AUDIT_MAX_PAGES);
    let mut text = String::new();
    let mut sampled_page_count = 0;

    for page_index in 0..pages_to_sample {
        if text.chars().count() >= AUDIT_MAX_CHARS {
            break;
        }

        let page_text = {
            let _lock = PDFIUM_LOCK.lock().unwrap();
            let page = document.pages().get(page_index as i32)?;
            let text_page = page.text()?;
            text_page.all()
        };

        sampled_page_count += 1;
        append_sample_text(&mut text, &page_text);
    }

    Ok(PdfAuditSample {
        text: cap_chars(text, AUDIT_MAX_CHARS),
        page_count: Some(page_count),
        sampled_page_count,
        degraded_fallback_used: false,
        warnings: Vec::new(),
    })
}

fn sample_with_lopdf_fallback(doc: &Document, warnings: Vec<String>) -> PdfAuditSample {
    let page_numbers: Vec<u32> = doc
        .get_pages()
        .keys()
        .copied()
        .take(AUDIT_MAX_PAGES)
        .collect();
    let page_count = doc.get_pages().len();
    let sampled_page_count = page_numbers.len();

    let mut warnings = warnings;
    let text = match doc.extract_text(&page_numbers) {
        Ok(text) => cap_chars(text, AUDIT_MAX_CHARS),
        Err(error) => {
            warnings.push(format!("Degraded lopdf fallback audit failed: {error}"));
            String::new()
        }
    };

    PdfAuditSample {
        text,
        page_count: Some(page_count),
        sampled_page_count,
        degraded_fallback_used: true,
        warnings,
    }
}

fn append_sample_text(sample: &mut String, page_text: &str) {
    if page_text.trim().is_empty() {
        return;
    }
    if !sample.is_empty() {
        sample.push('\n');
        sample.push('\n');
    }

    let remaining = AUDIT_MAX_CHARS.saturating_sub(sample.chars().count());
    sample.extend(page_text.chars().take(remaining));
}

fn cap_chars(text: String, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn classify_sample_text(text: &str) -> (PdfAuditQuality, usize, bool) {
    let embedded_text_chars = text.chars().count();
    let embedded_text_detected = text.chars().any(|c| !c.is_whitespace());
    if !embedded_text_detected {
        return (PdfAuditQuality::Empty, embedded_text_chars, false);
    }

    let (quality, _) = crate::pdf_quality::evaluate(text);
    (
        match quality {
            ExtractionQuality::Good => PdfAuditQuality::Good,
            ExtractionQuality::Suspicious => PdfAuditQuality::Suspicious,
            ExtractionQuality::Poor => PdfAuditQuality::Poor,
            ExtractionQuality::Empty => PdfAuditQuality::Empty,
        },
        embedded_text_chars,
        true,
    )
}

fn suggested_profile_for_quality(
    quality: &PdfAuditQuality,
    embedded_text_chars: usize,
) -> PdfAuditSuggestedProfile {
    match quality {
        PdfAuditQuality::Good => PdfAuditSuggestedProfile::Standard,
        PdfAuditQuality::Suspicious => PdfAuditSuggestedProfile::OcrRescue,
        PdfAuditQuality::Poor | PdfAuditQuality::Empty => PdfAuditSuggestedProfile::ForceOcr,
        PdfAuditQuality::Unknown => {
            if embedded_text_chars == 0 {
                PdfAuditSuggestedProfile::ForceOcr
            } else {
                PdfAuditSuggestedProfile::OcrRescue
            }
        }
    }
}

fn failed_result(
    path: String,
    file_name: String,
    pdfium_available: bool,
    ocr_model_resources_available: bool,
    warning: String,
) -> PdfAuditResult {
    PdfAuditResult {
        path,
        file_name,
        page_count: None,
        sampled_page_count: 0,
        embedded_text_detected: false,
        embedded_text_chars: 0,
        quality: PdfAuditQuality::Unknown,
        pdfium_available,
        ocr_model_resources_available,
        ocr_full_usability_checked: false,
        degraded_fallback_used: false,
        suggested_profile: PdfAuditSuggestedProfile::OcrRescue,
        warnings: vec![warning],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{Object, Stream, StringFormat, dictionary};
    use std::io::Write;

    macro_rules! require_pdfium {
        () => {
            if !crate::pdf_ocr::pdfium_available() {
                eprintln!("skipping PDFium-dependent audit test: PDFium library is not available");
                return;
            }
        };
    }

    #[test]
    fn suggested_profile_uses_quality_signal() {
        assert_eq!(
            suggested_profile_for_quality(&PdfAuditQuality::Good, 100),
            PdfAuditSuggestedProfile::Standard
        );
        assert_eq!(
            suggested_profile_for_quality(&PdfAuditQuality::Suspicious, 100),
            PdfAuditSuggestedProfile::OcrRescue
        );
        assert_eq!(
            suggested_profile_for_quality(&PdfAuditQuality::Empty, 0),
            PdfAuditSuggestedProfile::ForceOcr
        );
        assert_eq!(
            suggested_profile_for_quality(&PdfAuditQuality::Poor, 100),
            PdfAuditSuggestedProfile::ForceOcr
        );
    }

    #[test]
    fn invalid_pdf_returns_unknown_quality() {
        let mut file = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        file.write_all(b"This is not a PDF.").unwrap();

        let results = audit_pdf_files(vec![file.path().to_path_buf()]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].quality, PdfAuditQuality::Unknown);
        assert_eq!(results[0].page_count, None);
        assert!(
            results[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("Could not parse PDF for audit"))
        );
    }

    #[test]
    fn born_digital_pdf_reports_embedded_text_when_pdfium_available() {
        require_pdfium!();

        let mut file = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        file.write_all(&create_pdf_with_text(&[
            "This synthetic PDF contains ordinary embedded text for audit.",
            "The diagnostic should see this as selectable text.",
        ]))
        .unwrap();

        let results = audit_pdf_files(vec![file.path().to_path_buf()]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page_count, Some(1));
        assert!(results[0].embedded_text_detected);
        assert!(!results[0].degraded_fallback_used);
        assert_ne!(results[0].quality, PdfAuditQuality::Empty);
    }

    fn create_pdf_with_text(lines: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary!(
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica"
        ));

        let resources_id = doc.add_object(dictionary!(
            "Font" => dictionary!(
                "F1" => font_id
            )
        ));

        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
        ];

        let mut first = true;
        for line in lines {
            if first {
                ops.push(Operation::new("Td", vec![100.into(), 750.into()]));
                first = false;
            } else {
                ops.push(Operation::new("Td", vec![0.into(), (-30.0).into()]));
            }
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    line.as_bytes().to_vec(),
                    StringFormat::Literal,
                )],
            ));
        }
        ops.push(Operation::new("ET", vec![]));

        let content = Content { operations: ops };
        let content_id = doc.add_object(Stream::new(dictionary!(), content.encode().unwrap()));

        let page_id = doc.add_object(dictionary!(
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
        ));

        let pages = dictionary!(
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary!(
            "Type" => "Catalog",
            "Pages" => pages_id
        ));
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }
}
