use crate::clean::TableExtractionStrategy;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[derive(Debug)]
pub enum DocxError {
    InvalidZip,
    MissingDocumentXml,
    XmlParseError(String),
}

impl std::fmt::Display for DocxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocxError::InvalidZip => write!(f, "Invalid or corrupted DOCX (ZIP) archive"),
            DocxError::MissingDocumentXml => write!(f, "DOCX archive is missing word/document.xml"),
            DocxError::XmlParseError(e) => write!(f, "Failed to parse DOCX XML: {}", e),
        }
    }
}

impl std::error::Error for DocxError {}

/// Result of DOCX extraction.
#[derive(Debug)]
pub struct ExtractedDocx {
    pub text: String,
    pub warnings: Vec<String>,
}

/// Extracts text from a DOCX file byte slice.
pub fn extract_docx(
    bytes: &[u8],
    config: &crate::clean::CleaningConfig,
) -> Result<ExtractedDocx, DocxError> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|_| DocxError::InvalidZip)?;

    let mut header_files = Vec::new();
    let mut footer_files = Vec::new();
    let mut footnote_files = Vec::new();
    let mut endnote_files = Vec::new();
    let mut comment_files = Vec::new();

    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name().to_string();
            if name.starts_with("word/header") && name.ends_with(".xml") {
                header_files.push(name);
            } else if name.starts_with("word/footer") && name.ends_with(".xml") {
                footer_files.push(name);
            } else if name == "word/footnotes.xml" {
                footnote_files.push(name);
            } else if name == "word/endnotes.xml" {
                endnote_files.push(name);
            } else if name == "word/comments.xml" {
                comment_files.push(name);
            }
        }
    }

    header_files.sort();
    footer_files.sort();

    let mut files_to_parse = vec!["word/document.xml".to_string()];
    if !config.remove_headers {
        files_to_parse.extend(header_files);
    }
    if !config.remove_footers {
        files_to_parse.extend(footer_files);
    }
    if !config.remove_footnotes {
        files_to_parse.extend(footnote_files);
    }
    if !config.remove_endnotes {
        files_to_parse.extend(endnote_files);
    }
    if !config.remove_comments {
        files_to_parse.extend(comment_files);
    }

    let mut text = String::new();
    let mut warnings = Vec::new();

    for file_name in files_to_parse {
        let mut xml_bytes = Vec::new();
        {
            let mut file = match archive.by_name(&file_name) {
                Ok(f) => f,
                Err(_) => {
                    if file_name == "word/document.xml" {
                        return Err(DocxError::MissingDocumentXml);
                    }
                    continue;
                }
            };
            if file.read_to_end(&mut xml_bytes).is_err() {
                if file_name == "word/document.xml" {
                    return Err(DocxError::MissingDocumentXml);
                }
                continue;
            }
        }

        let xml_str = std::str::from_utf8(&xml_bytes).map_err(|e| {
            DocxError::XmlParseError(format!("Invalid UTF-8 in {}: {}", file_name, e))
        })?;

        let mut reader = Reader::from_str(xml_str);
        reader.config_mut().trim_text(false);
        reader.config_mut().expand_empty_elements = true;

        let mut in_text = false;
        let mut in_unsupported = false;
        let mut table_depth: usize = 0;
        let mut in_toc_p = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"w:tbl" => table_depth += 1,
                    b"w:t" => in_text = true,
                    b"w:commentRangeStart" | b"w:ins" | b"w:del" | b"w:drawing" => {
                        in_unsupported = true;
                        warnings.push(format!(
                            "Unsupported element encountered: <{}>",
                            String::from_utf8_lossy(e.name().as_ref())
                        ));
                    }
                    b"w:p" => in_toc_p = false,
                    b"w:pStyle" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                let val = String::from_utf8_lossy(&attr.value);
                                if val.starts_with("TOC") {
                                    in_toc_p = true;
                                }
                            }
                        }
                    }
                    b"w:tr" => {
                        if config.table_extraction_strategy == TableExtractionStrategy::TabSeparated
                            && !text.is_empty()
                            && !text.ends_with('\n')
                        {
                            text.push('\n');
                        }
                    }
                    b"w:br" => text.push('\n'),
                    b"w:tab" => text.push('\t'),
                    _ => (),
                },
                Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                    b"w:br" => text.push('\n'),
                    b"w:tab" => text.push('\t'),
                    b"w:pStyle" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w:val" {
                                let val = String::from_utf8_lossy(&attr.value);
                                if val.starts_with("TOC") {
                                    in_toc_p = true;
                                }
                            }
                        }
                    }
                    _ => (),
                },
                Ok(Event::Text(e)) => {
                    let ignoring_table = table_depth > 0
                        && config.table_extraction_strategy == TableExtractionStrategy::Ignore;
                    let ignoring_toc = in_toc_p && config.remove_table_of_contents;
                    if in_text && !in_unsupported && !ignoring_table && !ignoring_toc {
                        let unescaped = e
                            .unescape()
                            .map_err(|err| DocxError::XmlParseError(err.to_string()))?;
                        text.push_str(&unescaped);
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"w:t" => in_text = false,
                    b"w:commentRangeStart" | b"w:ins" | b"w:del" | b"w:drawing" => {
                        in_unsupported = false;
                    }
                    b"w:tc" => {
                        if config.table_extraction_strategy == TableExtractionStrategy::TabSeparated
                        {
                            text.push('\t');
                        } else if config.table_extraction_strategy
                            == TableExtractionStrategy::FlattenParagraphs
                            && !text.ends_with("\n\n")
                        {
                            if text.ends_with('\n') {
                                text.push('\n');
                            } else {
                                text.push_str("\n\n");
                            }
                        }
                    }
                    b"w:p" => {
                        if !text.ends_with("\n\n") {
                            if text.ends_with('\n') {
                                text.push('\n');
                            } else {
                                text.push_str("\n\n");
                            }
                        }
                        in_toc_p = false;
                    }
                    b"w:tr" => {
                        if config.table_extraction_strategy == TableExtractionStrategy::TabSeparated
                        {
                            text.push('\n');
                        }
                    }
                    b"w:tbl" if table_depth > 0 => {
                        table_depth = table_depth.saturating_sub(1);
                    }
                    _ => (),
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(DocxError::XmlParseError(e.to_string())),
                _ => (),
            }
        }

        // Add padding between files (e.g. between document and footnotes)
        if !text.is_empty() && !text.ends_with("\n\n") {
            if text.ends_with('\n') {
                text.push('\n');
            } else {
                text.push_str("\n\n");
            }
        }
    }

    let mut cleaned = if config.table_extraction_strategy == TableExtractionStrategy::TabSeparated {
        text.replace("\n\n\t", "\t").replace("\t\n", "\n")
    } else {
        text.clone()
    };

    cleaned = cleaned
        .lines()
        .map(|line| line.trim_end_matches('\t')) // removes trailing tabs on rows
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    while cleaned.contains("\n\n\n") {
        cleaned = cleaned.replace("\n\n\n", "\n\n");
    }

    warnings.sort();
    warnings.dedup();

    Ok(ExtractedDocx {
        text: cleaned,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::FileOptions;

    fn create_mock_docx(document_xml: &str, extras: Vec<(&str, &str)>) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut bytes));
            let options: FileOptions<'_, ()> = FileOptions::default();
            zip.start_file("word/document.xml", options).unwrap();
            zip.write_all(document_xml.as_bytes()).unwrap();
            for (name, content) in extras {
                zip.start_file(name, options).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        bytes
    }

    #[test]
    fn test_extract_paragraph_and_runs() {
        let xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:p>
                        <w:r><w:t>Hello </w:t></w:r>
                        <w:r><w:t>world</w:t></w:r>
                    </w:p>
                    <w:p>
                        <w:r><w:t>Second paragraph</w:t></w:r>
                    </w:p>
                </w:body>
            </w:document>
        "#;
        let bytes = create_mock_docx(xml, vec![]);
        let extracted = extract_docx(&bytes, &crate::clean::CleaningConfig::default()).unwrap();
        assert_eq!(extracted.text, "Hello world\n\nSecond paragraph");
        assert!(extracted.warnings.is_empty());
    }

    #[test]
    fn test_extract_table() {
        let xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:tbl>
                        <w:tr>
                            <w:tc><w:p><w:r><w:t>Header 1</w:t></w:r></w:p></w:tc>
                            <w:tc><w:p><w:r><w:t>Header 2</w:t></w:r></w:p></w:tc>
                        </w:tr>
                        <w:tr>
                            <w:tc><w:p><w:r><w:t>Cell 1</w:t></w:r></w:p></w:tc>
                            <w:tc><w:p><w:r><w:t>Cell 2</w:t></w:r></w:p></w:tc>
                        </w:tr>
                    </w:tbl>
                </w:body>
            </w:document>
        "#;
        let bytes = create_mock_docx(xml, vec![]);
        let extracted_tab = extract_docx(&bytes, &crate::clean::CleaningConfig::default()).unwrap();
        assert_eq!(extracted_tab.text, "Header 1\tHeader 2\nCell 1\tCell 2");

        let config_flat = crate::clean::CleaningConfig {
            table_extraction_strategy: TableExtractionStrategy::FlattenParagraphs,
            ..crate::clean::CleaningConfig::default()
        };
        let extracted_flat = extract_docx(&bytes, &config_flat).unwrap();
        assert_eq!(
            extracted_flat.text,
            "Header 1\n\nHeader 2\n\nCell 1\n\nCell 2"
        );

        let config_ignore = crate::clean::CleaningConfig {
            table_extraction_strategy: TableExtractionStrategy::Ignore,
            ..crate::clean::CleaningConfig::default()
        };
        let extracted_ignore = extract_docx(&bytes, &config_ignore).unwrap();
        assert_eq!(extracted_ignore.text, "");
    }

    #[test]
    fn test_xml_entities() {
        let xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:p>
                        <w:r><w:t>Tom &amp; Jerry &lt;classic&gt;</w:t></w:r>
                    </w:p>
                </w:body>
            </w:document>
        "#;
        let bytes = create_mock_docx(xml, vec![]);
        let extracted = extract_docx(&bytes, &crate::clean::CleaningConfig::default()).unwrap();
        assert_eq!(extracted.text, "Tom & Jerry <classic>");
    }

    #[test]
    fn test_extract_structured_elements() {
        let doc_xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:p><w:r><w:t>Main Document Content</w:t></w:r></w:p>
                </w:body>
            </w:document>
        "#;
        let header_xml = r#"
            <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:p><w:r><w:t>Header Content</w:t></w:r></w:p>
            </w:hdr>
        "#;
        let bytes = create_mock_docx(doc_xml, vec![("word/header1.xml", header_xml)]);

        let mut config = crate::clean::CleaningConfig {
            remove_headers: false,
            ..crate::clean::CleaningConfig::default()
        };
        let extracted = extract_docx(&bytes, &config).unwrap();
        assert_eq!(extracted.text, "Main Document Content\n\nHeader Content");

        config.remove_headers = true;
        let extracted_removed = extract_docx(&bytes, &config).unwrap();
        assert_eq!(extracted_removed.text, "Main Document Content");
    }

    #[test]
    fn test_extract_toc() {
        let doc_xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:p>
                        <w:pPr><w:pStyle w:val="TOC1"/></w:pPr>
                        <w:r><w:t>Table of Contents Item 1</w:t></w:r>
                    </w:p>
                    <w:p><w:r><w:t>Main Content</w:t></w:r></w:p>
                </w:body>
            </w:document>
        "#;
        let bytes = create_mock_docx(doc_xml, vec![]);

        let config = crate::clean::CleaningConfig::default();
        let extracted = extract_docx(&bytes, &config).unwrap();
        assert_eq!(extracted.text, "Table of Contents Item 1\n\nMain Content");

        let config = crate::clean::CleaningConfig {
            remove_table_of_contents: true,
            ..crate::clean::CleaningConfig::default()
        };
        let extracted_removed = extract_docx(&bytes, &config).unwrap();
        assert_eq!(extracted_removed.text, "Main Content");
    }

    #[test]
    fn test_br_and_tab() {
        let xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:p>
                        <w:r>
                            <w:t>Line 1</w:t>
                            <w:br/>
                            <w:tab/>
                            <w:t>Line 2</w:t>
                        </w:r>
                    </w:p>
                </w:body>
            </w:document>
        "#;
        let bytes = create_mock_docx(xml, vec![]);
        let extracted = extract_docx(&bytes, &crate::clean::CleaningConfig::default()).unwrap();
        assert_eq!(extracted.text, "Line 1\n\tLine 2");
    }

    #[test]
    fn test_invalid_zip() {
        let bytes = b"This is not a zip file";
        let err = extract_docx(bytes, &crate::clean::CleaningConfig::default()).unwrap_err();
        assert!(matches!(err, DocxError::InvalidZip));
    }

    #[test]
    fn test_missing_document_xml() {
        let mut bytes = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut bytes));
            let options: FileOptions<'_, ()> = FileOptions::default();
            zip.start_file("word/other.xml", options).unwrap();
            zip.write_all(b"<xml></xml>").unwrap();
            zip.finish().unwrap();
        }
        let err = extract_docx(&bytes, &crate::clean::CleaningConfig::default()).unwrap_err();
        assert!(matches!(err, DocxError::MissingDocumentXml));
    }

    #[test]
    fn test_unsupported_warnings() {
        let xml = r#"
            <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                <w:body>
                    <w:p>
                        <w:r><w:t>Normal text</w:t></w:r>
                        <w:drawing/>
                        <w:del/>
                    </w:p>
                </w:body>
            </w:document>
        "#;
        let bytes = create_mock_docx(xml, vec![]);
        let extracted = extract_docx(&bytes, &crate::clean::CleaningConfig::default()).unwrap();
        assert_eq!(extracted.text, "Normal text");
        assert_eq!(extracted.warnings.len(), 2);
        assert!(
            extracted
                .warnings
                .contains(&"Unsupported element encountered: <w:drawing>".to_string())
        );
        assert!(
            extracted
                .warnings
                .contains(&"Unsupported element encountered: <w:del>".to_string())
        );
    }
}
