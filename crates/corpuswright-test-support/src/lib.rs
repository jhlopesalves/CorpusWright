//! Deterministic, synthetic document fixtures shared by repository tests and tooling.
//!
//! This crate is deliberately absent from product runtime dependency graphs. Every document is
//! created in a temporary or explicitly supplied directory; no literary, academic, or copied web
//! text is embedded here.

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, StringFormat, dictionary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// The stable number of supported documents produced by [`generate_corpus_at`].
pub const SYNTHETIC_CORPUS_DOCUMENT_COUNT: usize = 8;

/// A tempfile-backed corpus whose directory remains alive for the value's lifetime.
#[derive(Debug)]
pub struct SyntheticCorpus {
    root: TempDir,
}

impl SyntheticCorpus {
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn document_count(&self) -> usize {
        SYNTHETIC_CORPUS_DOCUMENT_COUNT
    }
}

/// Creates the complete small cross-format corpus in an owned temporary directory.
pub fn synthetic_corpus() -> io::Result<SyntheticCorpus> {
    let root = tempfile::tempdir()?;
    generate_corpus_at(root.path())?;
    Ok(SyntheticCorpus { root })
}

/// Writes the stable browser/native/core fixture contract beneath `root`.
pub fn generate_corpus_at(root: &Path) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(root.join("nested"))?;
    let documents = [
        (
            PathBuf::from("clean.txt"),
            clean_text_fixture().as_bytes().to_vec(),
        ),
        (
            PathBuf::from("dirty.txt"),
            dirty_text_fixture().as_bytes().to_vec(),
        ),
        (
            PathBuf::from("nested/second.txt"),
            b"Synthetic field notes record amber signals. Amber signals recur predictably.\n"
                .to_vec(),
        ),
        (
            PathBuf::from("simple.html"),
            html_fixture().as_bytes().to_vec(),
        ),
        (
            PathBuf::from("nested/malformed.htm"),
            malformed_html_fixture().as_bytes().to_vec(),
        ),
        (PathBuf::from("basic.docx"), comprehensive_docx_bytes()?),
        (PathBuf::from("nested/tables.docx"), table_docx_bytes()?),
        (
            PathBuf::from("born-digital.pdf"),
            born_digital_pdf_bytes(&[
                "Synthetic PDF evidence uses invented vocabulary only.",
                "Amber quartz signals remain deterministic across runs.",
            ])?,
        ),
    ];

    let mut paths = Vec::with_capacity(documents.len());
    for (relative, bytes) in documents {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

pub fn clean_text_fixture() -> &'static str {
    "Synthetic observations remain stable.\n\nAmber tokens repeat in a controlled pattern.\n"
}

pub fn dirty_text_fixture() -> &'static str {
    "THIS IS UPPERCASE TEXT.\r\nPage 99\r\n--- Page 99 ---\r\nix\r\n1234\r\náéíóú\r\n\r\nSynthetic wrapped\r\nline.\r\n"
}

pub fn html_fixture() -> &'static str {
    "<!doctype html><html><head><style>.hidden{display:none}</style><script>ignored()</script></head><body><h1>Synthetic heading</h1><p>Amber &amp; quartz entities.</p><noscript>ignored fallback</noscript></body></html>"
}

pub fn malformed_html_fixture() -> &'static str {
    "<article><h1>Unclosed synthetic heading<p>Invented body &amp; entity<script>discard me"
}

pub fn html_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("simple.html", html_fixture()),
        ("malformed.html", malformed_html_fixture()),
        ("entities.html", "<p>&lt;amber&gt; &amp; &#35;synthetic</p>"),
        (
            "script-style.html",
            "<style>discard</style><p>Keep synthetic prose.</p><script>discard()</script>",
        ),
        ("fragment.htm", "<p>Small <b>invented</b> fragment.</p>"),
    ]
}

fn write_docx(entries: &[(&str, &str)]) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default();
        for (name, xml) in entries {
            zip.start_file(*name, options)?;
            zip.write_all(xml.as_bytes())?;
        }
        zip.finish()?;
    }
    Ok(bytes)
}

/// Exercises paragraphs, runs, formatting, breaks, tables, nesting, empty/merged cells,
/// headers, footers, notes, comments, and an unsupported drawing element.
pub fn comprehensive_docx_bytes() -> io::Result<Vec<u8>> {
    write_docx(&[
        (
            "word/document.xml",
            r#"<w:document xmlns:w="urn:w"><w:body>
<w:p><w:r><w:t>Synthetic paragraph</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t> with formatting</w:t><w:br/><w:tab/><w:t>after break</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>amber</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>merged quartz</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>
<w:tr><w:tc><w:tbl><w:tr><w:tc><w:p><w:r><w:t>nested cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>
<w:p><w:drawing><w:r><w:t>unsupported drawing text</w:t></w:r></w:drawing></w:p>
</w:body></w:document>"#,
        ),
        (
            "word/header1.xml",
            r#"<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>Synthetic header</w:t></w:r></w:p></w:hdr>"#,
        ),
        (
            "word/footer1.xml",
            r#"<w:ftr xmlns:w="urn:w"><w:p><w:r><w:t>Synthetic footer</w:t></w:r></w:p></w:ftr>"#,
        ),
        (
            "word/footnotes.xml",
            r#"<w:footnotes xmlns:w="urn:w"><w:footnote><w:p><w:r><w:t>Synthetic footnote</w:t></w:r></w:p></w:footnote></w:footnotes>"#,
        ),
        (
            "word/endnotes.xml",
            r#"<w:endnotes xmlns:w="urn:w"><w:endnote><w:p><w:r><w:t>Synthetic endnote</w:t></w:r></w:p></w:endnote></w:endnotes>"#,
        ),
        (
            "word/comments.xml",
            r#"<w:comments xmlns:w="urn:w"><w:comment><w:p><w:r><w:t>Synthetic comment</w:t></w:r></w:p></w:comment></w:comments>"#,
        ),
    ])
}

pub fn table_docx_bytes() -> io::Result<Vec<u8>> {
    write_docx(&[(
        "word/document.xml",
        r#"<w:document xmlns:w="urn:w"><w:body><w:tbl>
<w:tr><w:tc><w:p><w:r><w:t>row one</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>amber</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>row two</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>quartz</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl></w:body></w:document>"#,
    )])
}

pub fn malformed_docx_bytes() -> Vec<u8> {
    b"not a zip archive".to_vec()
}

pub fn born_digital_pdf_bytes(lines: &[&str]) -> io::Result<Vec<u8>> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary!(
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica"
    ));
    let resources_id = doc.add_object(dictionary!("Font" => dictionary!("F1" => font_id)));

    let mut operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 11.into()]),
    ];
    for (index, line) in lines.iter().enumerate() {
        operations.push(Operation::new(
            "Td",
            if index == 0 {
                vec![72.into(), 770.into()]
            } else {
                vec![0.into(), (-16).into()]
            },
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                line.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ));
    }
    operations.push(Operation::new("ET", vec![]));
    let content_id = doc.add_object(Stream::new(
        dictionary!(),
        Content { operations }.encode().map_err(io::Error::other)?,
    ));
    let page_id = doc.add_object(dictionary!(
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
    ));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary!(
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1
        )),
    );
    let catalog_id = doc.add_object(dictionary!("Type" => "Catalog", "Pages" => pages_id));
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes)
}

/// An image-only target with one blank page. OCR tests may render their own synthetic bitmap into
/// this contract when pixels are required; no copied image is embedded.
pub fn image_only_pdf_bytes() -> io::Result<Vec<u8>> {
    born_digital_pdf_bytes(&[])
}

pub fn malformed_pdf_bytes() -> Vec<u8> {
    b"%PDF-1.7\nsynthetic truncated object\n1 0 obj\n<<".to_vec()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticProjectManifest {
    pub schema_version: u32,
    pub name: String,
    pub language: String,
}

pub fn project_manifest() -> SyntheticProjectManifest {
    SyntheticProjectManifest {
        schema_version: 1,
        name: "Synthetic Research Project".to_string(),
        language: "en".to_string(),
    }
}

pub fn cache_envelope(payload: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "identity": "synthetic-cache-v1",
        "payload": payload
    })
}

pub fn repeated_artifact_input() -> &'static str {
    "SYNTHETIC RUNNING HEADER\nAmber evidence one.\n\nSYNTHETIC RUNNING HEADER\nQuartz evidence two.\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_contract_is_deterministic_and_complete() {
        let first = synthetic_corpus().unwrap();
        let second = synthetic_corpus().unwrap();
        let relative = |root: &Path| {
            let mut files = fs::read_dir(root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            files.sort();
            files
        };
        assert_eq!(first.document_count(), SYNTHETIC_CORPUS_DOCUMENT_COUNT);
        assert_eq!(relative(first.root()), relative(second.root()));
        assert_eq!(
            fs::read(first.root().join("born-digital.pdf")).unwrap(),
            fs::read(second.root().join("born-digital.pdf")).unwrap()
        );
    }

    #[test]
    fn malformed_inputs_are_deliberately_small_and_synthetic() {
        assert!(malformed_docx_bytes().len() < 100);
        assert!(malformed_pdf_bytes().len() < 100);
        assert!(!repeated_artifact_input().is_empty());
    }
}
