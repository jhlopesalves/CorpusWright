use chardetng::EncodingDetector;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::wrap_pyfunction;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Cursor, Read};
use unicode_segmentation::UnicodeSegmentation;
use walkdir::WalkDir;
use zip::ZipArchive;

fn read_preview(path: &str, limit: usize) -> Result<(String, bool), io::Error> {
    let mut file = File::open(path)?;

    if limit == 0 {
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        return Ok((buffer, false));
    }

    let mut buf = vec![0u8; limit + 1];
    let mut total_read = 0usize;
    while total_read < buf.len() {
        match file.read(&mut buf[total_read..]) {
            Ok(0) => break,
            Ok(n) => total_read += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    buf.truncate(total_read);

    let truncated = buf.len() > limit;
    if truncated {
        buf.truncate(limit);
    }

    let text = String::from_utf8_lossy(&buf).to_string();
    Ok((text, truncated))
}

fn decode_with_chardet(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        text.into_owned()
    }
}

fn is_html_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"))
        .unwrap_or(false)
}

fn is_pdf_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

fn is_supported_scan_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("txt")
        || ext.eq_ignore_ascii_case("html")
        || ext.eq_ignore_ascii_case("htm")
        || ext.eq_ignore_ascii_case("docx")
        || ext.eq_ignore_ascii_case("pdf")
}

fn extract_html_text_from_bytes(bytes: &[u8]) -> PyResult<String> {
    let decoded = decode_with_chardet(bytes);
    html2text::from_read(decoded.as_bytes(), 120)
        .map_err(|err| PyValueError::new_err(err.to_string()))
}

fn extract_file_text(path: &str) -> PyResult<(String, u64)> {
    let data = fs::read(path).map_err(|err| PyIOError::new_err(err.to_string()))?;
    let text = if is_html_path(path) {
        extract_html_text_from_bytes(&data)?
    } else if is_pdf_path(path) {
        extract_pdf_text_from_path(path)?.text
    } else if std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("docx"))
        .unwrap_or(false)
    {
        extract_docx_text_from_bytes(&data)?
    } else {
        decode_with_chardet(&data)
    };
    Ok((text, data.len() as u64))
}

fn count_words(text: &str, use_simple_split: bool) -> u64 {
    if use_simple_split {
        text.split_whitespace()
            .filter(|segment| !segment.is_empty())
            .count() as u64
    } else {
        UnicodeSegmentation::unicode_words(text).count() as u64
    }
}

#[pyfunction]
fn load_preview(path: &str, limit: usize) -> PyResult<(String, bool)> {
    if path.is_empty() {
        return Err(PyValueError::new_err("Path cannot be empty"));
    }

    read_preview(path, limit).map_err(|err| PyIOError::new_err(err.to_string()))
}

#[pyfunction]
fn scan_directory(path: &str) -> PyResult<Vec<String>> {
    if path.is_empty() {
        return Err(PyValueError::new_err("Directory path cannot be empty"));
    }

    let walker = WalkDir::new(path).into_iter();
    let mut results = Vec::new();

    for entry in walker {
        let entry = entry.map_err(|err| PyIOError::new_err(err.to_string()))?;
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if is_supported_scan_extension(ext) {
                    results.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }

    Ok(results)
}

#[pyfunction]
#[pyo3(signature = (paths, use_processed, word_tokenization, processed_lookup=None, progress_callback=None))]
fn generate_report_summary(
    py: Python<'_>,
    paths: Vec<String>,
    use_processed: bool,
    word_tokenization: bool,
    processed_lookup: Option<HashMap<String, String>>,
    progress_callback: Option<PyObject>,
) -> PyResult<Py<PyDict>> {
    let total_files = paths.len();
    let mut total_bytes: u64 = 0;
    let mut total_words: u64 = 0;

    let processed_lookup = processed_lookup.unwrap_or_default();
    let progress_callback = progress_callback.as_ref();

    if total_files == 0 {
        if let Some(callback) = progress_callback {
            callback.call1(py, (100,))?;
        }
    }

    for (idx, path) in paths.iter().enumerate() {
        let (text, bytes): (Cow<'_, str>, u64) = if use_processed {
            if let Some(processed_text) = processed_lookup.get(path) {
                (
                    Cow::Borrowed(processed_text.as_str()),
                    processed_text.as_bytes().len() as u64,
                )
            } else {
                let (text, bytes) = extract_file_text(path)?;
                (Cow::Owned(text), bytes)
            }
        } else {
            let (text, bytes) = extract_file_text(path)?;
            (Cow::Owned(text), bytes)
        };

        total_bytes += bytes;
        total_words += count_words(&text, word_tokenization);

        if let Some(callback) = progress_callback {
            let percent = if total_files == 0 {
                100
            } else {
                ((idx + 1) * 100 / total_files) as i32
            };
            callback.call1(py, (percent,))?;
        }
    }

    let dict = PyDict::new_bound(py);
    dict.set_item("total_files", total_files)?;
    let total_size_mb = total_bytes as f64 / (1024.0 * 1024.0);
    dict.set_item("total_size", total_size_mb)?;
    let avg_size_mb = if total_files > 0 {
        total_size_mb / total_files as f64
    } else {
        0.0
    };
    dict.set_item("avg_size", avg_size_mb)?;
    dict.set_item("total_words", total_words)?;
    let avg_words = if total_files > 0 {
        total_words as f64 / total_files as f64
    } else {
        0.0
    };
    dict.set_item("avg_words", avg_words)?;

    Ok(dict.into())
}

#[pyfunction]
fn load_full_text(path: &str) -> PyResult<String> {
    if path.is_empty() {
        return Err(PyValueError::new_err("Path cannot be empty"));
    }

    let data = fs::read(path).map_err(|err| PyIOError::new_err(err.to_string()))?;
    Ok(decode_with_chardet(&data))
}

#[pyfunction]
fn load_full_texts(paths: Vec<String>) -> PyResult<Vec<(String, String)>> {
    let mut results = Vec::with_capacity(paths.len());

    for path in paths {
        if path.is_empty() {
            return Err(PyValueError::new_err("Path cannot be empty"));
        }

        let data = fs::read(&path).map_err(|err| PyIOError::new_err(err.to_string()))?;
        let text = decode_with_chardet(&data);
        results.push((path, text));
    }

    Ok(results)
}

#[pyfunction]
fn extract_html_text(path: &str) -> PyResult<String> {
    if path.is_empty() {
        return Err(PyValueError::new_err("Path cannot be empty"));
    }

    let data = fs::read(path).map_err(|err| PyIOError::new_err(err.to_string()))?;
    extract_html_text_from_bytes(&data)
}

fn extract_docx_text_from_bytes(bytes: &[u8]) -> PyResult<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|err| PyValueError::new_err(format!("Invalid DOCX file: {err}")))?;
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| PyValueError::new_err("Invalid DOCX file: missing word/document.xml"))?
        .read_to_string(&mut document_xml)
        .map_err(|err| PyIOError::new_err(err.to_string()))?;

    extract_docx_text_from_xml(&document_xml)
}

fn extract_docx_text_from_xml(document_xml: &str) -> PyResult<String> {
    let mut reader = Reader::from_str(document_xml);
    reader.config_mut().trim_text(false);

    let mut output_lines: Vec<String> = Vec::new();
    let mut current_paragraph = String::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell_paragraphs: Vec<String> = Vec::new();
    let mut inside_paragraph = false;
    let mut inside_cell = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref event)) => match local_name(event.name().as_ref()) {
                b"p" => {
                    inside_paragraph = true;
                    current_paragraph.clear();
                }
                b"tc" => {
                    inside_cell = true;
                    current_cell_paragraphs.clear();
                }
                b"tr" => {
                    current_row.clear();
                }
                b"tab" if inside_paragraph => current_paragraph.push('\t'),
                b"br" | b"cr" if inside_paragraph => current_paragraph.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(ref event)) => match local_name(event.name().as_ref()) {
                b"tab" if inside_paragraph => current_paragraph.push('\t'),
                b"br" | b"cr" if inside_paragraph => current_paragraph.push('\n'),
                _ => {}
            },
            Ok(Event::Text(event)) => {
                if inside_paragraph {
                    let text = event.unescape().map_err(|err| {
                        PyValueError::new_err(format!(
                            "Invalid DOCX XML in word/document.xml: {err}"
                        ))
                    })?;
                    current_paragraph.push_str(&text);
                }
            }
            Ok(Event::End(ref event)) => match local_name(event.name().as_ref()) {
                b"p" => {
                    inside_paragraph = false;
                    let paragraph = normalize_docx_paragraph(&current_paragraph);
                    if !paragraph.is_empty() {
                        if inside_cell {
                            current_cell_paragraphs.push(paragraph);
                        } else {
                            output_lines.push(paragraph);
                        }
                    }
                    current_paragraph.clear();
                }
                b"tc" => {
                    inside_cell = false;
                    let cell_text = current_cell_paragraphs.join("\n");
                    if !cell_text.is_empty() {
                        current_row.push(cell_text);
                    }
                    current_cell_paragraphs.clear();
                }
                b"tr" => {
                    if !current_row.is_empty() {
                        output_lines.push(current_row.join(" | "));
                    }
                    current_row.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(err) => {
                return Err(PyValueError::new_err(format!(
                    "Invalid DOCX XML in word/document.xml: {err}"
                )))
            }
            _ => {}
        }
    }

    Ok(output_lines.join("\n").trim().to_string())
}

fn normalize_docx_paragraph(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    lines.join("\n").trim().to_string()
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

#[pyfunction]
fn extract_docx_text(path: &str) -> PyResult<String> {
    if path.is_empty() {
        return Err(PyValueError::new_err("Path cannot be empty"));
    }

    let data = fs::read(path).map_err(|err| PyIOError::new_err(err.to_string()))?;
    extract_docx_text_from_bytes(&data)
}

#[derive(Debug)]
struct PdfExtractionResult {
    text: String,
    page_count: usize,
    image_count: usize,
}

fn extract_pdf_text_from_path(path: &str) -> PyResult<PdfExtractionResult> {
    let document = pdf_extract::Document::load(path).map_err(pdf_load_error_to_py)?;
    if document.is_encrypted() {
        return Err(PyValueError::new_err(
            "Encrypted/password-protected PDF is unsupported",
        ));
    }

    let page_count = document.get_pages().len();
    let image_count = count_pdf_images(&document);
    let pages = pdf_extract::extract_text_by_pages(path).map_err(pdf_extract_error_to_py)?;
    let text = pages.join("\n\x0c\n").trim().to_string();

    Ok(PdfExtractionResult {
        text,
        page_count: if page_count == 0 {
            pages.len()
        } else {
            page_count
        },
        image_count,
    })
}

fn count_pdf_images(document: &pdf_extract::Document) -> usize {
    let mut image_count = 0usize;
    for page_id in document.get_pages().values() {
        if let Ok(images) = document.get_page_images(*page_id) {
            image_count += images.len();
        }
    }
    image_count
}

fn pdf_load_error_to_py<E: std::fmt::Display>(err: E) -> PyErr {
    let message = err.to_string();
    if pdf_error_message_looks_encrypted(&message) {
        PyValueError::new_err(format!(
            "Encrypted/password-protected PDF is unsupported: {message}"
        ))
    } else {
        PyValueError::new_err(format!("Invalid PDF file: {message}"))
    }
}

fn pdf_extract_error_to_py<E: std::fmt::Display>(err: E) -> PyErr {
    let message = err.to_string();
    if pdf_error_message_looks_encrypted(&message) {
        PyValueError::new_err(format!(
            "Encrypted/password-protected PDF is unsupported: {message}"
        ))
    } else {
        PyValueError::new_err(format!("PDF extraction failed: {message}"))
    }
}

fn pdf_error_message_looks_encrypted(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("encrypt") || lower.contains("password") || lower.contains("decrypt")
}

#[pyfunction]
fn extract_pdf_text(py: Python<'_>, path: &str) -> PyResult<(String, Py<PyDict>)> {
    if path.is_empty() {
        return Err(PyValueError::new_err("Path cannot be empty"));
    }

    let result = extract_pdf_text_from_path(path)?;
    let metadata = PyDict::new_bound(py);
    metadata.set_item("backend", "pdf-extract")?;
    metadata.set_item("native_backend", true)?;
    metadata.set_item("page_count", result.page_count)?;
    metadata.set_item("image_count", result.image_count)?;
    metadata.set_item("page_separator", "form-feed")?;
    Ok((result.text, metadata.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn supported_scan_extensions_include_current_ingestion_formats() {
        assert!(is_supported_scan_extension("txt"));
        assert!(is_supported_scan_extension("HTML"));
        assert!(is_supported_scan_extension("htm"));
        assert!(is_supported_scan_extension("DOCX"));
        assert!(is_supported_scan_extension("PDF"));
        assert!(!is_supported_scan_extension("doc"));
    }

    #[test]
    fn docx_xml_extractor_handles_paragraphs_and_tables() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Rust paragraph.</w:t></w:r></w:p>
    <w:tbl>
      <w:tr>
        <w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc>
        <w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

        let text = extract_docx_text_from_xml(document_xml).unwrap();

        assert_eq!(text, "Rust paragraph.\nCell A | Cell B");
    }

    #[test]
    fn pdf_extractor_handles_minimal_embedded_text_pdf() {
        let path = unique_pdf_path("text");
        std::fs::write(&path, minimal_pdf_bytes("Rust PDF text")).unwrap();

        let extraction = extract_pdf_text_from_path(path.to_str().unwrap()).unwrap();

        assert!(extraction.text.contains("Rust PDF text"));
        assert_eq!(extraction.page_count, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pdf_extractor_reports_empty_page_without_text() {
        let path = unique_pdf_path("empty");
        std::fs::write(&path, minimal_pdf_bytes("")).unwrap();

        let extraction = extract_pdf_text_from_path(path.to_str().unwrap()).unwrap();

        assert!(extraction.text.trim().is_empty());
        assert_eq!(extraction.page_count, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pdf_extractor_rejects_invalid_pdf_bytes() {
        pyo3::prepare_freethreaded_python();
        let path = unique_pdf_path("invalid");
        std::fs::write(&path, b"not a pdf").unwrap();

        let error = extract_pdf_text_from_path(path.to_str().unwrap()).unwrap_err();

        assert!(error.to_string().contains("Invalid PDF file"));
        let _ = std::fs::remove_file(path);
    }

    fn unique_pdf_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "corpus_preview_{label}_{}_{}.pdf",
            std::process::id(),
            nanos
        ))
    }

    fn minimal_pdf_bytes(text: &str) -> Vec<u8> {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        let content = if escaped.is_empty() {
            String::new()
        } else {
            format!("BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET\n")
        };
        let objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{}endstream",
                content.as_bytes().len(),
                content
            ),
        ];

        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![0usize];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                offsets.len(),
                xref_offset
            )
            .as_bytes(),
        );
        pdf
    }
}

#[pymodule]
fn corpus_preview(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load_preview, m)?)?;
    m.add_function(wrap_pyfunction!(scan_directory, m)?)?;
    m.add_function(wrap_pyfunction!(generate_report_summary, m)?)?;
    m.add_function(wrap_pyfunction!(load_full_text, m)?)?;
    m.add_function(wrap_pyfunction!(load_full_texts, m)?)?;
    m.add_function(wrap_pyfunction!(extract_html_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_docx_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_pdf_text, m)?)?;
    Ok(())
}
