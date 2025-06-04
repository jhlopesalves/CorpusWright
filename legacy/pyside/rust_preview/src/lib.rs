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
fn is_supported_scan_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("txt")
        || ext.eq_ignore_ascii_case("html")
        || ext.eq_ignore_ascii_case("htm")
        || ext.eq_ignore_ascii_case("docx")
        || ext.eq_ignore_ascii_case("pdf")
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
#[pymodule]
fn corpus_preview(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load_preview, m)?)?;
    m.add_function(wrap_pyfunction!(scan_directory, m)?)?;
    m.add_function(wrap_pyfunction!(load_full_text, m)?)?;
    m.add_function(wrap_pyfunction!(load_full_texts, m)?)?;
    Ok(())
}
