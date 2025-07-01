import hashlib
import json
import os
import subprocess
import sys
import urllib.error
from pathlib import Path
import xml.etree.ElementTree as ET
from zipfile import ZipFile

import pytest

from corpusaid.ingestion import (
    DocxExtractor,
    DocumentIngestionService,
    ExtractionBackendResult,
    ExtractionFallbackPolicy,
    ExtractedDocument,
    ExtractionManifest,
    ExtractionWarning,
    HtmlExtractor,
    PdfExtractor,
    PlainTextExtractor,
    RustTextBackend,
    TikaServerBackend,
    UnsupportedDocumentTypeError,
    can_overwrite_with_extracted_text,
    supported_document_formats,
    supported_file_extensions,
)


WORD_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"

ET.register_namespace("w", WORD_NS)


def plain_text_extractor():
    return PlainTextExtractor(rust_backend=RustTextBackend())


def html_extractor():
    return HtmlExtractor(rust_backend=RustTextBackend())


def docx_extractor():
    return DocxExtractor(rust_backend=RustTextBackend())


def pdf_extractor(rust_backend=None, fallback_backend=None, fallback_policy=None):
    return PdfExtractor(
        rust_backend=rust_backend or RustTextBackend(),
        fallback_backend=fallback_backend,
        fallback_policy=fallback_policy,
    )


class FakeFallbackBackend:
    backend_name = "tika:server"
    support_status = "experimental"

    def __init__(self, text="", success=True, warnings=None):
        self.text = text
        self.success = success
        self.warnings = list(warnings or [])
        self.calls = []

    def extract(self, path: Path, *, fallback_reason: str):
        self.calls.append((path, fallback_reason))
        return ExtractionBackendResult(
            backend_name=self.backend_name,
            success=self.success,
            text=self.text,
            warnings=list(self.warnings),
            metadata={"backend": "tika", "fallback_reason": fallback_reason},
            support_status=self.support_status,
        )


def write_docx(
    path: Path,
    document_xml: str,
    *,
    comments: bool = False,
    footnotes: bool = False,
    endnotes: bool = False,
    headers: bool = False,
    footers: bool = False,
):
    with ZipFile(path, "w") as archive:
        archive.writestr(
            "[Content_Types].xml",
            (
                '<?xml version="1.0" encoding="UTF-8"?>'
                '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                '<Default Extension="xml" ContentType="application/xml"/>'
                '<Override PartName="/word/document.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
                "</Types>"
            ),
        )
        archive.writestr(
            "_rels/.rels",
            (
                '<?xml version="1.0" encoding="UTF-8"?>'
                '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                '<Relationship Id="rId1" '
                'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
                'Target="word/document.xml"/>'
                "</Relationships>"
            ),
        )
        archive.writestr("word/document.xml", document_xml)
        if comments:
            archive.writestr("word/comments.xml", "<comments/>")
        if footnotes:
            archive.writestr("word/footnotes.xml", "<footnotes/>")
        if endnotes:
            archive.writestr("word/endnotes.xml", "<endnotes/>")
        if headers:
            archive.writestr("word/header1.xml", "<hdr/>")
        if footers:
            archive.writestr("word/footer1.xml", "<ftr/>")


def simple_docx_xml(body: str) -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        f"<w:body>{body}</w:body>"
        "</w:document>"
    )


def write_generated_docx(
    path: Path,
    *,
    paragraphs=None,
    table_rows=None,
):
    document = ET.Element(f"{{{WORD_NS}}}document")
    body = ET.SubElement(document, f"{{{WORD_NS}}}body")

    for text in paragraphs or []:
        paragraph = ET.SubElement(body, f"{{{WORD_NS}}}p")
        run = ET.SubElement(paragraph, f"{{{WORD_NS}}}r")
        text_node = ET.SubElement(run, f"{{{WORD_NS}}}t")
        text_node.text = text

    if table_rows:
        table = ET.SubElement(body, f"{{{WORD_NS}}}tbl")
        for row_values in table_rows:
            row = ET.SubElement(table, f"{{{WORD_NS}}}tr")
            for value in row_values:
                cell = ET.SubElement(row, f"{{{WORD_NS}}}tc")
                paragraph = ET.SubElement(cell, f"{{{WORD_NS}}}p")
                run = ET.SubElement(paragraph, f"{{{WORD_NS}}}r")
                text_node = ET.SubElement(run, f"{{{WORD_NS}}}t")
                text_node.text = value

    sect_pr = ET.SubElement(body, f"{{{WORD_NS}}}sectPr")
    ET.SubElement(
        sect_pr,
        f"{{{WORD_NS}}}pgSz",
        {f"{{{WORD_NS}}}w": "12240", f"{{{WORD_NS}}}h": "15840"},
    )
    document_xml = ET.tostring(document, encoding="utf-8", xml_declaration=True)

    with ZipFile(path, "w") as archive:
        archive.writestr(
            "[Content_Types].xml",
            (
                '<?xml version="1.0" encoding="UTF-8"?>'
                '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                '<Default Extension="xml" ContentType="application/xml"/>'
                '<Override PartName="/word/document.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
                '<Override PartName="/word/styles.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
                '<Override PartName="/docProps/core.xml" '
                'ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>'
                '<Override PartName="/docProps/app.xml" '
                'ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>'
                "</Types>"
            ),
        )
        archive.writestr(
            "_rels/.rels",
            (
                '<?xml version="1.0" encoding="UTF-8"?>'
                f'<Relationships xmlns="{REL_NS}">'
                '<Relationship Id="rId1" '
                'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
                'Target="word/document.xml"/>'
                '<Relationship Id="rId2" '
                'Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" '
                'Target="docProps/core.xml"/>'
                '<Relationship Id="rId3" '
                'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" '
                'Target="docProps/app.xml"/>'
                "</Relationships>"
            ),
        )
        archive.writestr(
            "word/_rels/document.xml.rels",
            (
                '<?xml version="1.0" encoding="UTF-8"?>'
                f'<Relationships xmlns="{REL_NS}"/>'
            ),
        )
        archive.writestr("word/document.xml", document_xml)
        archive.writestr(
            "word/styles.xml",
            (
                '<?xml version="1.0" encoding="UTF-8"?>'
                f'<w:styles xmlns:w="{WORD_NS}"/>'
            ),
        )
        archive.writestr(
            "docProps/core.xml",
            (
                '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"/>'
            ),
        )
        archive.writestr(
            "docProps/app.xml",
            (
                '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"/>'
            ),
        )


def test_ingestion_package_does_not_import_pyside6():
    script = (
        "import corpusaid.ingestion, sys; "
        "raise SystemExit(any(name.startswith('PySide6') for name in sys.modules))"
    )
    src_path = str(Path(__file__).resolve().parents[1] / "src")
    env = dict(os.environ)
    env["PYTHONPATH"] = (
        src_path
        if not env.get("PYTHONPATH")
        else os.pathsep.join([src_path, env["PYTHONPATH"]])
    )
    result = subprocess.run([sys.executable, "-c", script], check=False, env=env)

    assert result.returncode == 0


def test_extracts_utf8_txt_file(tmp_path):
    path = tmp_path / "sample.txt"
    path.write_text("Hello, corpus.\nCafe com acento.", encoding="utf-8")

    document = plain_text_extractor().extract(path)

    assert document.source_path == path
    assert document.text == "Hello, corpus.\nCafe com acento."
    assert document.document_type == "txt"
    assert document.extraction_method == "python:utf-8"
    assert document.warnings == []
    assert document.metadata["extension"] == ".txt"
    assert document.metadata["size_bytes"] == path.stat().st_size


def test_supports_uppercase_txt_extension(tmp_path):
    path = tmp_path / "SAMPLE.TXT"
    path.write_text("Uppercase extension", encoding="utf-8")

    document = plain_text_extractor().extract(str(path))

    assert document.source_path == path
    assert document.text == "Uppercase extension"
    assert document.document_type == "txt"


def test_extracts_empty_txt_file(tmp_path):
    path = tmp_path / "empty.txt"
    path.write_text("", encoding="utf-8")

    document = plain_text_extractor().extract(path)

    assert document.text == ""
    assert document.extraction_method == "python:utf-8"
    assert document.warnings == []
    assert document.metadata["size_bytes"] == 0


def test_missing_txt_file_raises_file_not_found(tmp_path):
    path = tmp_path / "missing.txt"

    with pytest.raises(FileNotFoundError):
        plain_text_extractor().extract(path)


def test_extracts_invalid_utf8_txt_with_replacement_warning(tmp_path):
    path = tmp_path / "latin1-ish.txt"
    path.write_bytes(b"valid text \xff still loaded")

    document = plain_text_extractor().extract(path)

    assert document.text == "valid text \ufffd still loaded"
    assert document.extraction_method == "python:utf-8-replace"
    assert len(document.warnings) == 1
    warning = document.warnings[0]
    assert isinstance(warning, ExtractionWarning)
    assert warning.code == "invalid_utf8_replacement"
    assert warning.message == "Invalid UTF-8 bytes were replaced while reading text"
    assert "0xff" in str(warning)


def test_unsupported_extension_raises(tmp_path):
    path = tmp_path / "sample.docx"
    path.write_bytes(b"not extracted yet")
    service = DocumentIngestionService([plain_text_extractor()])

    assert not service.supports(path)
    with pytest.raises(UnsupportedDocumentTypeError):
        service.extract(path)


def test_supported_file_extensions_include_text_and_html():
    assert supported_file_extensions() == [".docx", ".htm", ".html", ".pdf", ".txt"]


def test_supported_document_formats_include_support_levels():
    assert supported_document_formats() == [
        {
            "document_type": "docx",
            "extensions": [".docx"],
            "support_level": "experimental",
            "can_overwrite_with_extracted_text": False,
        },
        {
            "document_type": "html",
            "extensions": [".htm", ".html"],
            "support_level": "stable",
            "can_overwrite_with_extracted_text": False,
        },
        {
            "document_type": "pdf",
            "extensions": [".pdf"],
            "support_level": "experimental",
            "can_overwrite_with_extracted_text": False,
        },
        {
            "document_type": "txt",
            "extensions": [".txt"],
            "support_level": "stable",
            "can_overwrite_with_extracted_text": True,
        },
    ]


def test_supported_extensions_are_derived_from_registered_extractors():
    class CustomExtractor:
        supported_extensions = {"MD", ".RST", ""}

        def supports(self, path):
            return Path(path).suffix.lower() in {".md", ".rst"}

        def extract(self, path):
            raise NotImplementedError

    service = DocumentIngestionService([plain_text_extractor(), CustomExtractor()])

    assert service.supported_extensions() == [".md", ".rst", ".txt"]


def test_overwrite_safety_is_derived_from_registered_extractor():
    service = DocumentIngestionService(
        [plain_text_extractor(), html_extractor(), docx_extractor()]
    )

    assert service.can_overwrite_with_extracted_text("sample.txt") is True
    assert service.can_overwrite_with_extracted_text("sample.html") is False
    assert service.can_overwrite_with_extracted_text("sample.docx") is False
    assert service.can_overwrite_with_extracted_text("sample.pdf") is False
    assert service.can_overwrite_with_extracted_text("sample.md") is False
    assert can_overwrite_with_extracted_text("sample.txt") is True


def test_ingestion_service_chooses_matching_extractor(tmp_path):
    class MarkdownExtractor:
        def supports(self, path):
            return Path(path).suffix.lower() == ".md"

        def extract(self, path):
            source_path = Path(path)
            return ExtractedDocument(
                source_path=source_path,
                text="markdown extracted",
                document_type="md",
                extraction_method="test:markdown",
            )

    text_path = tmp_path / "sample.txt"
    text_path.write_text("plain text", encoding="utf-8")
    markdown_path = tmp_path / "sample.md"
    markdown_path.write_text("# heading", encoding="utf-8")

    service = DocumentIngestionService([MarkdownExtractor(), plain_text_extractor()])

    assert service.extract(text_path).document_type == "txt"
    assert service.extract(markdown_path).document_type == "md"


def test_docx_extraction_with_simple_paragraphs(tmp_path):
    path = tmp_path / "sample.docx"
    write_docx(
        path,
        simple_docx_xml(
            "<w:p><w:r><w:t>First paragraph.</w:t></w:r></w:p>"
            "<w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p>"
        ),
        comments=True,
        headers=True,
    )

    document = docx_extractor().extract(path)

    assert document.document_type == "docx"
    assert document.extraction_method == "python:zip+xml"
    assert document.text == "First paragraph.\nSecond paragraph."
    assert document.metadata["extension"] == ".docx"
    assert document.metadata["has_tables"] is False
    assert document.metadata["unextracted_parts"] == ["comments", "headers"]
    assert any(warning.code == "rust_docx_unavailable" for warning in document.warnings)
    assert any(warning.code == "docx_unextracted_parts" for warning in document.warnings)


def test_generated_docx_fixture_extracts_multiple_non_ascii_paragraphs(tmp_path):
    path = tmp_path / "realistic-paragraphs.docx"
    write_generated_docx(
        path,
        paragraphs=[
            "Primeiro paragrafo com acentos: ação, coração, João.",
            "Segundo parágrafo com unicode: café, naïve, 你好.",
        ],
    )

    document = docx_extractor().extract(path)

    assert document.text == (
        "Primeiro paragrafo com acentos: ação, coração, João.\n"
        "Segundo parágrafo com unicode: café, naïve, 你好."
    )
    assert document.metadata["has_tables"] is False


def test_generated_docx_fixture_extracts_simple_table(tmp_path):
    path = tmp_path / "realistic-table.docx"
    write_generated_docx(
        path,
        paragraphs=["Before the table."],
        table_rows=[
            ["Header A", "Header B"],
            ["Linha 1", "Valor 1"],
        ],
    )

    document = docx_extractor().extract(path)

    assert document.text == "Before the table.\nHeader A | Header B\nLinha 1 | Valor 1"
    assert document.metadata["has_tables"] is True


def test_docx_supports_uppercase_extension(tmp_path):
    path = tmp_path / "SAMPLE.DOCX"
    write_docx(
        path,
        simple_docx_xml("<w:p><w:r><w:t>Uppercase DOCX</w:t></w:r></w:p>"),
    )

    document = docx_extractor().extract(str(path))

    assert document.source_path == path
    assert document.text == "Uppercase DOCX"


def test_docx_extracts_table_text(tmp_path):
    path = tmp_path / "table.docx"
    write_docx(
        path,
        simple_docx_xml(
            "<w:tbl>"
            "<w:tr>"
            "<w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc>"
            "<w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc>"
            "</w:tr>"
            "</w:tbl>"
        ),
    )

    document = docx_extractor().extract(path)

    assert document.text == "Cell A | Cell B"
    assert document.metadata["has_tables"] is True


def test_docx_tracked_changes_emit_warning(tmp_path):
    path = tmp_path / "tracked.docx"
    write_docx(
        path,
        simple_docx_xml(
            "<w:p><w:r><w:t>Kept</w:t></w:r><w:ins><w:r><w:t>Inserted</w:t></w:r></w:ins></w:p>"
        ),
    )

    document = docx_extractor().extract(path)

    assert "Inserted" in document.text
    assert any(
        warning.code == "docx_tracked_changes_not_extracted"
        for warning in document.warnings
    )


def test_docx_warnings_cover_unextracted_package_parts(tmp_path):
    path = tmp_path / "extra-parts.docx"
    write_docx(
        path,
        simple_docx_xml("<w:p><w:r><w:t>Main body</w:t></w:r></w:p>"),
        comments=True,
        footnotes=True,
        endnotes=True,
        headers=True,
        footers=True,
    )

    document = docx_extractor().extract(path)

    assert document.metadata["unextracted_parts"] == [
        "comments",
        "footnotes",
        "endnotes",
        "headers",
        "footers",
    ]
    warning = next(
        warning
        for warning in document.warnings
        if warning.code == "docx_unextracted_parts"
    )
    assert warning.details == "comments, footnotes, endnotes, headers, footers"
    assert "current DOCX support" in warning.message


def test_empty_docx_body_warns(tmp_path):
    path = tmp_path / "empty.docx"
    write_docx(path, simple_docx_xml(""))

    document = docx_extractor().extract(path)

    assert document.text == ""
    assert any(
        warning.code == "empty_docx_extraction" for warning in document.warnings
    )


def test_missing_docx_file_raises_file_not_found(tmp_path):
    path = tmp_path / "missing.docx"

    with pytest.raises(FileNotFoundError):
        docx_extractor().extract(path)


def test_invalid_docx_raises_value_error(tmp_path):
    path = tmp_path / "broken.docx"
    path.write_bytes(b"not a zip archive")

    with pytest.raises(ValueError, match="Invalid DOCX file"):
        docx_extractor().extract(path)


def test_missing_document_xml_raises_value_error(tmp_path):
    path = tmp_path / "missing-document.docx"
    with ZipFile(path, "w") as archive:
        archive.writestr("[Content_Types].xml", "<Types/>")

    with pytest.raises(ValueError, match="missing word/document.xml"):
        docx_extractor().extract(path)


def test_doc_extension_remains_unsupported(tmp_path):
    path = tmp_path / "legacy.doc"
    path.write_bytes(b"legacy binary doc")

    service = DocumentIngestionService([plain_text_extractor(), html_extractor(), docx_extractor()])

    assert not service.supports(path)
    with pytest.raises(UnsupportedDocumentTypeError):
        service.extract(path)


def test_ingestion_service_chooses_docx_extractor(tmp_path):
    path = tmp_path / "sample.docx"
    write_docx(
        path,
        simple_docx_xml("<w:p><w:r><w:t>Dispatched DOCX</w:t></w:r></w:p>"),
    )
    service = DocumentIngestionService(
        [plain_text_extractor(), html_extractor(), docx_extractor()]
    )

    document = service.extract(path)

    assert document.document_type == "docx"
    assert document.text == "Dispatched DOCX"


def test_pdf_supports_uppercase_extension_without_native_backend(tmp_path):
    path = tmp_path / "SAMPLE.PDF"
    path.write_bytes(b"%PDF-1.4\n")

    document = pdf_extractor().extract(str(path))

    assert document.source_path == path
    assert document.document_type == "pdf"
    assert document.text == ""
    assert document.extraction_method == "unavailable:corpus_preview.extract_pdf_text"
    assert document.metadata["extension"] == ".pdf"
    assert document.metadata["source_type"] == "pdf"
    assert document.metadata["support_status"] == "experimental"
    assert document.metadata["backend"] == "pdf-extract"
    assert document.metadata["native_backend"] is False
    assert document.metadata["extracted_character_count"] == 0
    assert [warning.code for warning in document.warnings] == [
        "rust_pdf_unavailable",
        "fallback_not_configured",
        "empty_pdf_extraction",
    ]
    assert document.metadata["primary_backend"] == "unavailable:corpus_preview.extract_pdf_text"
    assert document.metadata["chosen_backend"] == "unavailable:corpus_preview.extract_pdf_text"
    assert document.metadata["fallback_attempted"] is False
    assert document.metadata["fallback_reason"] == "primary_failed"
    assert len(document.metadata["backend_attempts"]) == 1


def test_pdf_native_success_uses_rust_metadata_and_page_boundaries(tmp_path):
    path = tmp_path / "native.pdf"
    path.write_bytes(b"%PDF-1.4\n")

    def fake_pdf_extractor(_path: str):
        return (
            "First page has enough embedded PDF text for quality checks.\n\f\nSecond page also has enough embedded text.",
            {
                "backend": "pdf-extract",
                "native_backend": True,
                "page_count": 2,
                "page_separator": "form-feed",
            },
        )

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=fake_pdf_extractor)
    ).extract(path)

    assert document.extraction_method == "rust:corpus_preview.extract_pdf_text"
    assert "First page has enough embedded PDF text" in document.text
    assert document.metadata["page_count"] == 2
    assert document.metadata["page_separator"] == "form-feed"
    assert document.metadata["native_backend"] is True
    assert document.metadata["extracted_character_count"] == len(document.text)
    assert document.metadata["fallback_attempted"] is False
    assert document.metadata["fallback_reason"] is None
    assert document.metadata["chosen_backend"] == "rust:corpus_preview.extract_pdf_text"
    assert document.warnings == []


def test_pdf_native_failure_records_structured_warnings_without_fallback(tmp_path):
    path = tmp_path / "broken.pdf"
    path.write_bytes(b"not really a pdf")

    def broken_pdf_extractor(_path: str):
        raise RuntimeError("PDF error: trailer not found")

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=broken_pdf_extractor)
    ).extract(path)

    assert document.text == ""
    assert [warning.code for warning in document.warnings] == [
        "pdf_extraction_failed",
        "fallback_not_configured",
        "empty_pdf_extraction",
    ]
    assert "trailer not found" in document.warnings[0].details
    assert document.metadata["fallback_reason"] == "primary_failed"


def test_encrypted_pdf_failure_uses_specific_warning(tmp_path):
    path = tmp_path / "encrypted.pdf"
    path.write_bytes(b"%PDF-1.4\n")

    def encrypted_pdf_extractor(_path: str):
        raise RuntimeError("Encrypted/password-protected PDF is unsupported")

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=encrypted_pdf_extractor)
    ).extract(path)

    assert document.text == ""
    assert document.warnings[0].code == "encrypted_pdf_unsupported"
    assert any(
        warning.code == "fallback_not_configured" for warning in document.warnings
    )


def test_pdf_empty_image_like_result_warns_about_scanned_pdf(tmp_path):
    path = tmp_path / "image-only.pdf"
    path.write_bytes(b"%PDF-1.4\n")

    def image_only_pdf_extractor(_path: str):
        return (
            "",
            {
                "backend": "pdf-extract",
                "native_backend": True,
                "page_count": 1,
                "image_count": 1,
            },
        )

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=image_only_pdf_extractor)
    ).extract(path)

    warning_codes = [warning.code for warning in document.warnings]
    assert "fallback_not_configured" in warning_codes
    assert "empty_pdf_extraction" in warning_codes
    assert "pdf_suspected_scanned_or_image_only" in warning_codes
    assert document.metadata["page_count"] == 1
    assert document.metadata["image_count"] == 1
    assert document.metadata["fallback_reason"] == "primary_empty"


def test_pdf_native_success_does_not_attempt_available_fallback(tmp_path):
    path = tmp_path / "native.pdf"
    path.write_bytes(b"%PDF-1.4\n")
    fallback = FakeFallbackBackend(text="Fallback text should not be used")

    def fake_pdf_extractor(_path: str):
        return (
            "Native PDF extraction produced enough text to avoid fallback.",
            {"backend": "pdf-extract", "native_backend": True, "page_count": 1},
        )

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=fake_pdf_extractor),
        fallback_backend=fallback,
    ).extract(path)

    assert fallback.calls == []
    assert document.text.startswith("Native PDF extraction")
    assert document.metadata["fallback_available"] is True
    assert document.metadata["fallback_attempted"] is False
    assert document.metadata["chosen_backend"] == "rust:corpus_preview.extract_pdf_text"
    assert len(document.metadata["backend_attempts"]) == 1


def test_pdf_native_failure_uses_fake_fallback(tmp_path):
    path = tmp_path / "fallback.pdf"
    path.write_bytes(b"not really a pdf")
    fallback = FakeFallbackBackend(text="Fallback Tika text from server")

    def broken_pdf_extractor(_path: str):
        raise RuntimeError("native pdf failed")

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=broken_pdf_extractor),
        fallback_backend=fallback,
    ).extract(path)

    assert fallback.calls[0] == (path, "primary_failed")
    assert document.text == "Fallback Tika text from server"
    assert document.extraction_method == "tika:server"
    assert document.metadata["backend"] == "tika"
    assert document.metadata["primary_backend"] == "rust:corpus_preview.extract_pdf_text"
    assert document.metadata["chosen_backend"] == "tika:server"
    assert document.metadata["fallback_attempted"] is True
    assert document.metadata["fallback_reason"] == "primary_failed"
    assert len(document.metadata["backend_attempts"]) == 2
    assert [warning.code for warning in document.warnings] == [
        "pdf_extraction_failed",
        "fallback_attempted",
        "fallback_used",
        "near_empty_pdf_extraction",
    ]


def test_pdf_empty_native_result_uses_fake_fallback(tmp_path):
    path = tmp_path / "empty.pdf"
    path.write_bytes(b"%PDF-1.4\n")
    fallback = FakeFallbackBackend(text="Fallback text for empty native extraction")

    def empty_pdf_extractor(_path: str):
        return ("", {"backend": "pdf-extract", "native_backend": True, "page_count": 1})

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=empty_pdf_extractor),
        fallback_backend=fallback,
    ).extract(path)

    assert fallback.calls[0] == (path, "primary_empty")
    assert document.text == "Fallback text for empty native extraction"
    assert document.metadata["fallback_attempted"] is True
    assert document.metadata["fallback_reason"] == "primary_empty"
    assert document.metadata["chosen_backend"] == "tika:server"


def test_pdf_near_empty_native_result_uses_fake_fallback(tmp_path):
    path = tmp_path / "near-empty.pdf"
    path.write_bytes(b"%PDF-1.4\n")
    fallback = FakeFallbackBackend(text="Fallback text for near-empty extraction")

    def near_empty_pdf_extractor(_path: str):
        return ("tiny", {"backend": "pdf-extract", "native_backend": True, "page_count": 1})

    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=near_empty_pdf_extractor),
        fallback_backend=fallback,
    ).extract(path)

    assert fallback.calls[0] == (path, "primary_near_empty")
    assert document.text == "Fallback text for near-empty extraction"
    assert document.metadata["fallback_reason"] == "primary_near_empty"


def test_tika_fallback_unavailable_does_not_break_pdf_extraction(tmp_path):
    path = tmp_path / "tika-unavailable.pdf"
    path.write_bytes(b"not really a pdf")

    def broken_pdf_extractor(_path: str):
        raise RuntimeError("native pdf failed")

    def unavailable_urlopen(_request, timeout):
        raise urllib.error.URLError("connection refused")

    fallback = TikaServerBackend(
        "http://127.0.0.1:9998",
        urlopen=unavailable_urlopen,
    )
    document = pdf_extractor(
        RustTextBackend(extract_pdf_text=broken_pdf_extractor),
        fallback_backend=fallback,
    ).extract(path)

    warning_codes = [warning.code for warning in document.warnings]
    assert document.text == ""
    assert "fallback_attempted" in warning_codes
    assert "tika_fallback_unavailable" in warning_codes
    assert "empty_pdf_extraction" in warning_codes
    assert document.metadata["fallback_attempted"] is True
    assert document.metadata["chosen_backend"] == "rust:corpus_preview.extract_pdf_text"


def test_tika_server_backend_success_uses_put_text_plain(tmp_path):
    path = tmp_path / "server.pdf"
    path.write_bytes(b"%PDF-1.4\n")
    captured = {}

    class FakeResponse:
        status = 200

        def __enter__(self):
            return self

        def __exit__(self, _exc_type, _exc, _traceback):
            return False

        def read(self):
            return b"Tika extracted text"

    def fake_urlopen(request, timeout):
        captured["url"] = request.full_url
        captured["method"] = request.get_method()
        captured["accept"] = request.get_header("Accept")
        captured["content_type"] = request.get_header("Content-type")
        captured["timeout"] = timeout
        captured["data"] = request.data
        return FakeResponse()

    backend = TikaServerBackend(
        "http://localhost:9998",
        timeout_seconds=3,
        urlopen=fake_urlopen,
    )

    result = backend.extract(path, fallback_reason="primary_failed")

    assert result.success is True
    assert result.text == "Tika extracted text"
    assert result.backend_name == "tika:server"
    assert result.metadata["fallback_reason"] == "primary_failed"
    assert captured["url"] == "http://localhost:9998/tika"
    assert captured["method"] == "PUT"
    assert captured["accept"] == "text/plain"
    assert captured["content_type"] == "application/pdf"
    assert captured["timeout"] == 3
    assert captured["data"] == b"%PDF-1.4\n"


def test_tika_fallback_is_disabled_without_environment(monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)

    assert TikaServerBackend.from_environment() is None


def test_html_extraction_with_simple_paragraph(tmp_path):
    path = tmp_path / "sample.html"
    path.write_text("<html><body><p>Hello <strong>HTML</strong>.</p></body></html>")

    document = html_extractor().extract(path)

    assert document.document_type == "html"
    assert document.extraction_method == "python:html.parser"
    assert document.text == "Hello HTML."
    assert document.warnings[0].code == "rust_html_unavailable"


def test_htm_extraction(tmp_path):
    path = tmp_path / "sample.htm"
    path.write_text("<html><body><h1>Heading</h1><p>Body</p></body></html>")

    document = html_extractor().extract(path)

    assert document.text == "Heading\nBody"
    assert document.metadata["extension"] == ".htm"


def test_html_extraction_supports_uppercase_extension(tmp_path):
    path = tmp_path / "SAMPLE.HTML"
    path.write_text("<html><body><p>Uppercase HTML</p></body></html>")

    document = html_extractor().extract(str(path))

    assert document.source_path == path
    assert document.text == "Uppercase HTML"


def test_html_title_metadata(tmp_path):
    path = tmp_path / "titled.html"
    path.write_text(
        "<html><head><title> Test Title </title></head>"
        "<body><p>Text</p></body></html>"
    )

    document = html_extractor().extract(path)

    assert document.metadata["title"] == "Test Title"


def test_html_ignores_script_style_and_noscript_content(tmp_path):
    path = tmp_path / "noise.html"
    path.write_text(
        "<html><head><style>.hidden{display:none}</style></head><body>"
        "<script>alert('bad')</script><noscript>fallback</noscript>"
        "<p>Visible text</p></body></html>"
    )

    document = html_extractor().extract(path)

    assert document.text == "Visible text"
    assert "alert" not in document.text
    assert "display" not in document.text
    assert "fallback" not in document.text


def test_malformed_html_is_recovered_with_warning(tmp_path):
    path = tmp_path / "malformed.html"
    path.write_text("<html><body><p>First <strong>bold<p>Second</body></html>")

    document = html_extractor().extract(path)

    assert "First bold" in document.text
    assert "Second" in document.text
    assert any(
        warning.code == "malformed_html_recovered"
        for warning in document.warnings
    )


def test_empty_html_body_warning(tmp_path):
    path = tmp_path / "empty.html"
    path.write_text("<html><head><title>Empty</title></head><body></body></html>")

    document = html_extractor().extract(path)

    assert document.text == ""
    assert document.metadata["title"] == "Empty"
    assert any(
        warning.code == "empty_html_extraction"
        for warning in document.warnings
    )


def test_ingestion_service_chooses_html_extractor(tmp_path):
    path = tmp_path / "sample.html"
    path.write_text("<html><body><p>Dispatched</p></body></html>")
    service = DocumentIngestionService([plain_text_extractor(), html_extractor()])

    document = service.extract(path)

    assert document.document_type == "html"
    assert document.text == "Dispatched"


def test_html_rust_unavailable_uses_python_fallback(tmp_path):
    path = tmp_path / "fallback.html"
    path.write_text("<html><body><p>Fallback</p></body></html>")

    document = HtmlExtractor(rust_backend=RustTextBackend()).extract(path)

    assert document.extraction_method == "python:html.parser"
    assert document.text == "Fallback"
    assert document.warnings[0].code == "rust_html_unavailable"


def test_html_rust_failure_uses_python_fallback(tmp_path):
    path = tmp_path / "fallback.html"
    path.write_text("<html><body><p>Fallback</p></body></html>")

    def broken_html_extractor(_path: str) -> str:
        raise RuntimeError("native html unavailable")

    document = HtmlExtractor(
        rust_backend=RustTextBackend(extract_html_text=broken_html_extractor)
    ).extract(path)

    assert document.extraction_method == "python:html.parser"
    assert document.text == "Fallback"
    assert document.warnings[0].code == "rust_html_fallback"
    assert "native html unavailable" in str(document.warnings[0])


def test_html_rust_success_uses_native_text_with_python_title_metadata(tmp_path):
    path = tmp_path / "native.html"
    path.write_text("<html><head><title>Native</title></head><body>ignored</body></html>")

    document = HtmlExtractor(
        rust_backend=RustTextBackend(extract_html_text=lambda _path: "Native text")
    ).extract(path)

    assert document.extraction_method == "rust:corpus_preview.extract_html_text"
    assert document.text == "Native text"
    assert document.metadata["title"] == "Native"


def test_docx_rust_failure_uses_python_fallback(tmp_path):
    path = tmp_path / "fallback.docx"
    write_docx(
        path,
        simple_docx_xml("<w:p><w:r><w:t>Fallback DOCX</w:t></w:r></w:p>"),
    )

    def broken_docx_extractor(_path: str) -> str:
        raise RuntimeError("native docx unavailable")

    document = DocxExtractor(
        rust_backend=RustTextBackend(extract_docx_text=broken_docx_extractor)
    ).extract(path)

    assert document.extraction_method == "python:zip+xml"
    assert document.text == "Fallback DOCX"
    assert document.warnings[0].code == "rust_docx_fallback"
    assert "native docx unavailable" in str(document.warnings[0])


def test_docx_rust_success_uses_native_text_with_python_metadata(tmp_path):
    path = tmp_path / "native.docx"
    write_docx(
        path,
        simple_docx_xml("<w:p><w:r><w:t>Ignored</w:t></w:r></w:p>"),
        footnotes=True,
    )

    document = DocxExtractor(
        rust_backend=RustTextBackend(extract_docx_text=lambda _path: "Native DOCX text")
    ).extract(path)

    assert document.extraction_method == "rust:corpus_preview.extract_docx_text"
    assert document.text == "Native DOCX text"
    assert document.metadata["unextracted_parts"] == ["footnotes"]


def test_extraction_manifest_serializes_to_json_dict(tmp_path):
    path = tmp_path / "manifest.txt"
    path.write_text("Manifest text", encoding="utf-8")
    document = ExtractedDocument(
        source_path=path,
        text="Manifest text",
        document_type="txt",
        extraction_method="python:utf-8",
        warnings=[
            ExtractionWarning(
                code="sample_warning",
                message="Sample warning",
                details="details",
            )
        ],
        metadata={"extension": ".txt", "nested": {"path": path}},
    )

    manifest = ExtractionManifest.from_document(
        document,
        app_version="0.1.0",
        project_version="project-2026",
        include_source_hash=True,
    )
    payload = manifest.to_json_dict()

    assert payload["app_version"] == "0.1.0"
    assert payload["project_version"] == "project-2026"
    assert payload["source_path"] == str(path)
    assert payload["document_type"] == "txt"
    assert payload["extraction_method"] == "python:utf-8"
    assert payload["extracted_character_count"] == len("Manifest text")
    assert payload["warnings"] == [
        {
            "code": "sample_warning",
            "message": "Sample warning",
            "details": "details",
        }
    ]
    assert payload["metadata"]["nested"]["path"] == str(path)
    assert payload["source_file_hash"] == {
        "algorithm": "sha256",
        "value": hashlib.sha256(b"Manifest text").hexdigest(),
    }
    json.dumps(payload)


def test_extraction_manifest_hash_is_optional(tmp_path):
    path = tmp_path / "manifest.txt"
    document = ExtractedDocument(
        source_path=path,
        text="No hash",
        document_type="txt",
        extraction_method="python:utf-8",
    )

    payload = ExtractionManifest.from_document(document).to_json_dict()

    assert payload["source_file_hash"] is None
    assert payload["app_version"] is None
    assert payload["project_version"] is None


def test_preview_loading_truncates_without_caching_full_file(tmp_path):
    path = tmp_path / "sample.txt"
    path.write_text("abcdef", encoding="utf-8")

    preview = plain_text_extractor().preview(path, 3)

    assert preview.text == "abc"
    assert preview.truncated is True
    assert preview.document_type == "txt"
    assert preview.extraction_method == "python:utf-8"
    assert preview.warnings == []


def test_preview_loading_collects_invalid_byte_warning(tmp_path):
    path = tmp_path / "sample.txt"
    path.write_bytes(b"abc\xffdef")

    preview = plain_text_extractor().preview(path, 20)

    assert preview.text == "abc\ufffddef"
    assert preview.truncated is False
    assert preview.extraction_method == "python:utf-8-replace"
    assert preview.warnings[0].code == "invalid_utf8_preview_replacement"


def test_rust_loader_failure_warning_is_collected(tmp_path):
    path = tmp_path / "sample.txt"
    path.write_text("Fallback text", encoding="utf-8")

    def broken_loader(_path: str) -> str:
        raise RuntimeError("native loader unavailable")

    extractor = PlainTextExtractor(
        rust_backend=RustTextBackend(load_full_text=broken_loader)
    )

    document = extractor.extract(Path(path))

    assert document.text == "Fallback text"
    assert document.extraction_method == "python:utf-8"
    assert document.warnings[0].code == "rust_full_text_fallback"
    assert "native loader unavailable" in str(document.warnings[0])
