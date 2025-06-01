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
#[pymodule]
fn corpus_preview(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load_preview, m)?)?;
    m.add_function(wrap_pyfunction!(scan_directory, m)?)?;
    m.add_function(wrap_pyfunction!(load_full_text, m)?)?;
    m.add_function(wrap_pyfunction!(load_full_texts, m)?)?;
    m.add_function(wrap_pyfunction!(extract_html_text, m)?)?;
    m.add_function(wrap_pyfunction!(extract_docx_text, m)?)?;
    Ok(())
}
