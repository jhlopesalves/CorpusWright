import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET
from zipfile import ZipFile

from corpusaid.exporting import ProcessedDocumentRecord, export_processed_corpus
from corpusaid.ingestion import (
    DocxExtractor,
    ExtractedDocument,
    ExtractionWarning,
    HtmlExtractor,
    PdfExtractor,
    PlainTextExtractor,
    RustTextBackend,
)


WORD_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"

ET.register_namespace("w", WORD_NS)


def plain_text_extractor():
    return PlainTextExtractor(rust_backend=RustTextBackend())


def html_extractor():
    return HtmlExtractor(rust_backend=RustTextBackend())


def docx_extractor():
    return DocxExtractor(rust_backend=RustTextBackend())


def pdf_extractor(rust_backend=None):
    return PdfExtractor(
        rust_backend=rust_backend or RustTextBackend(),
        fallback_backend=None,
    )


def write_docx(path: Path, text: str):
    document = ET.Element(f"{{{WORD_NS}}}document")
    body = ET.SubElement(document, f"{{{WORD_NS}}}body")
    paragraph = ET.SubElement(body, f"{{{WORD_NS}}}p")
    run = ET.SubElement(paragraph, f"{{{WORD_NS}}}r")
    text_node = ET.SubElement(run, f"{{{WORD_NS}}}t")
    text_node.text = text
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
                "</Types>"
            ),
        )
        archive.writestr("word/document.xml", document_xml)


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def single_document_manifest(export_dir: Path):
    manifest = read_json(export_dir / "manifest.json")
    return manifest, manifest["documents"][0]


def exported_text_path(export_dir: Path, document_payload):
    return export_dir / document_payload["output_text_path"]


def test_exports_one_processed_txt_with_manifest_structure(tmp_path):
    source = tmp_path / "sample.txt"
    source.write_text("Original text", encoding="utf-8")
    extracted = plain_text_extractor().extract(source)

    result = export_processed_corpus(
        [ProcessedDocumentRecord.from_extracted_document(extracted, "Processed text")],
        {"lowercase": True},
        tmp_path / "export",
        app_version="0.8",
    )
    manifest, document = single_document_manifest(result.output_directory)

    assert result.files_exported == 1
    assert result.warning_count == 0
    assert (result.output_directory / "texts").is_dir()
    assert result.config_path.name == "config.json"
    assert result.warnings_path.name == "warnings.json"
    assert result.warnings_csv_path.name == "warnings.csv"
    assert result.readme_path.name == "README.txt"
    assert exported_text_path(result.output_directory, document).read_text(
        encoding="utf-8"
    ) == "Processed text"
    assert document["output_text_path"].startswith("texts/sample-")
    assert document["output_text_path"].endswith(".txt")
    assert document["source_path"] == str(source)
    assert document["source_extension"] == ".txt"
    assert document["document_type"] == "txt"
    assert document["support_status"] == "stable"
    assert document["extraction_method"] == "python:utf-8"
    assert document["structured_source"] is False
    assert document["extracted_character_count"] == len("Original text")
    assert document["processed_character_count"] == len("Processed text")
    assert manifest["app_name"] == "CorpusAid"
    assert manifest["app_version"] == "0.8"
    assert manifest["files_exported"] == 1
    assert manifest["warning_count"] == 0
    assert manifest["processing_config"] == {"lowercase": True}
    assert manifest["export_timestamp"].endswith("Z")


def test_exports_one_processed_html_as_txt(tmp_path):
    source = tmp_path / "page.html"
    source.write_text(
        "<html><body><p>Original HTML text</p></body></html>",
        encoding="utf-8",
    )
    extracted = html_extractor().extract(source)

    result = export_processed_corpus(
        [ProcessedDocumentRecord.from_extracted_document(extracted, "Processed HTML")],
        {},
        tmp_path / "export",
    )
    _, document = single_document_manifest(result.output_directory)

    assert exported_text_path(result.output_directory, document).suffix == ".txt"
    assert exported_text_path(result.output_directory, document).read_text(
        encoding="utf-8"
    ) == "Processed HTML"
    assert document["document_type"] == "html"
    assert document["support_status"] == "stable"
    assert document["structured_source"] is True


def test_exports_one_experimental_docx_as_txt(tmp_path):
    source = tmp_path / "chapter.docx"
    write_docx(source, "Original DOCX text")
    extracted = docx_extractor().extract(source)

    result = export_processed_corpus(
        [ProcessedDocumentRecord.from_extracted_document(extracted, "Processed DOCX")],
        {},
        tmp_path / "export",
    )
    _, document = single_document_manifest(result.output_directory)

    assert exported_text_path(result.output_directory, document).suffix == ".txt"
    assert document["document_type"] == "docx"
    assert document["support_status"] == "experimental"
    assert document["structured_source"] is True
    assert "experimental" == document["support_status"]


def test_exports_one_experimental_pdf_as_txt_without_overwriting_source(tmp_path):
    source = tmp_path / "paper.pdf"
    source.write_bytes(b"%PDF-1.4\n")

    def fake_pdf_extractor(_path: str):
        return (
            "Original PDF text with enough embedded text to avoid fallback diagnostics.",
            {
                "backend": "pdf-extract",
                "native_backend": True,
                "page_count": 1,
            },
        )

    extracted = pdf_extractor(
        RustTextBackend(extract_pdf_text=fake_pdf_extractor)
    ).extract(source)
    original_bytes = source.read_bytes()

    result = export_processed_corpus(
        [ProcessedDocumentRecord.from_extracted_document(extracted, "Processed PDF")],
        {},
        tmp_path / "export",
    )
    _, document = single_document_manifest(result.output_directory)

    output_path = exported_text_path(result.output_directory, document)
    assert output_path.suffix == ".txt"
    assert output_path != source
    assert output_path.read_text(encoding="utf-8") == "Processed PDF"
    assert source.read_bytes() == original_bytes
    assert document["source_extension"] == ".pdf"
    assert document["document_type"] == "pdf"
    assert document["support_status"] == "experimental"
    assert document["structured_source"] is True
    assert document["metadata"]["backend"] == "pdf-extract"
    assert document["metadata"]["page_count"] == 1
    assert document["metadata"]["primary_backend"] == "rust:corpus_preview.extract_pdf_text"
    assert document["metadata"]["chosen_backend"] == "rust:corpus_preview.extract_pdf_text"
    assert document["metadata"]["fallback_attempted"] is False
    assert len(document["metadata"]["backend_attempts"]) == 1


def test_pdf_warnings_are_included_in_manifest_and_warning_files(tmp_path):
    source = tmp_path / "image-only.pdf"
    source.write_bytes(b"%PDF-1.4\n")

    def image_only_pdf_extractor(_path: str):
        return (
            "",
            {
                "backend": "pdf-extract",
                "native_backend": True,
                "page_count": 1,
            },
        )

    extracted = pdf_extractor(
        RustTextBackend(extract_pdf_text=image_only_pdf_extractor)
    ).extract(source)

    result = export_processed_corpus(
        [ProcessedDocumentRecord.from_extracted_document(extracted, "")],
        {},
        tmp_path / "export",
    )
    manifest, document_payload = single_document_manifest(result.output_directory)
    warnings_payload = read_json(result.warnings_path)
    warnings_csv = result.warnings_csv_path.read_text(encoding="utf-8")
    codes = [warning["code"] for warning in document_payload["warnings"]]

    assert result.warning_count == 3
    assert manifest["warning_count"] == 3
    assert codes == [
        "fallback_not_configured",
        "empty_pdf_extraction",
        "pdf_suspected_scanned_or_image_only",
    ]
    assert warnings_payload["warnings"][0]["code"] == "fallback_not_configured"
    assert "fallback_not_configured" in warnings_csv
    assert "empty_pdf_extraction" in warnings_csv
    assert "pdf_suspected_scanned_or_image_only" in warnings_csv
    assert document_payload["metadata"]["fallback_reason"] == "primary_empty"
    assert document_payload["metadata"]["fallback_attempted"] is False
    assert document_payload["metadata"]["chosen_backend"] == "rust:corpus_preview.extract_pdf_text"


def test_duplicate_basenames_from_different_directories_do_not_overwrite(tmp_path):
    first = tmp_path / "a" / "sample.txt"
    second = tmp_path / "b" / "sample.txt"
    first.parent.mkdir()
    second.parent.mkdir()
    first.write_text("First source", encoding="utf-8")
    second.write_text("Second source", encoding="utf-8")
    records = [
        ProcessedDocumentRecord.from_extracted_document(
            plain_text_extractor().extract(first), "First processed"
        ),
        ProcessedDocumentRecord.from_extracted_document(
            plain_text_extractor().extract(second), "Second processed"
        ),
    ]

    result = export_processed_corpus(records, {}, tmp_path / "export")
    manifest = read_json(result.manifest_path)
    output_paths = [
        exported_text_path(result.output_directory, document)
        for document in manifest["documents"]
    ]

    assert len(output_paths) == 2
    assert len({path.name for path in output_paths}) == 2
    assert all(path.name.startswith("sample-") for path in output_paths)
    assert [path.read_text(encoding="utf-8") for path in output_paths] == [
        "First processed",
        "Second processed",
    ]


def test_duplicate_pdf_basenames_export_to_distinct_txt_files(tmp_path):
    first = tmp_path / "a" / "paper.pdf"
    second = tmp_path / "b" / "paper.pdf"
    first.parent.mkdir()
    second.parent.mkdir()
    first.write_bytes(b"%PDF-1.4 first\n")
    second.write_bytes(b"%PDF-1.4 second\n")

    def fake_pdf_extractor(path: str):
        return (f"PDF text from {Path(path).parent.name}", {"page_count": 1})

    extractor = pdf_extractor(RustTextBackend(extract_pdf_text=fake_pdf_extractor))
    records = [
        ProcessedDocumentRecord.from_extracted_document(
            extractor.extract(first), "First processed PDF"
        ),
        ProcessedDocumentRecord.from_extracted_document(
            extractor.extract(second), "Second processed PDF"
        ),
    ]

    result = export_processed_corpus(records, {}, tmp_path / "export")
    manifest = read_json(result.manifest_path)
    output_paths = [
        exported_text_path(result.output_directory, document)
        for document in manifest["documents"]
    ]

    assert len(output_paths) == 2
    assert len({path.name for path in output_paths}) == 2
    assert all(path.suffix == ".txt" for path in output_paths)
    assert all(path.name.startswith("paper-") for path in output_paths)
    assert [path.read_text(encoding="utf-8") for path in output_paths] == [
        "First processed PDF",
        "Second processed PDF",
    ]


def test_warnings_are_included_in_manifest_and_warning_files(tmp_path):
    source = tmp_path / "warned.txt"
    source.write_text("Original", encoding="utf-8")
    document = ExtractedDocument(
        source_path=source,
        text="Original",
        document_type="txt",
        extraction_method="test:extract",
        warnings=[
            ExtractionWarning(
                code="sample_warning",
                message="Sample warning",
                details="details",
            )
        ],
    )

    result = export_processed_corpus(
        [ProcessedDocumentRecord.from_extracted_document(document, "Processed")],
        {},
        tmp_path / "export",
    )
    manifest, document_payload = single_document_manifest(result.output_directory)
    warnings_payload = read_json(result.warnings_path)
    warnings_csv = result.warnings_csv_path.read_text(encoding="utf-8")

    assert result.warning_count == 1
    assert manifest["warning_count"] == 1
    assert document_payload["warnings"] == [
        {
            "code": "sample_warning",
            "message": "Sample warning",
            "details": "details",
        }
    ]
    assert warnings_payload["warnings"][0]["code"] == "sample_warning"
    assert "sample_warning" in warnings_csv
    assert "Sample warning" in warnings_csv


def test_config_is_written_to_config_json_and_manifest(tmp_path):
    source = tmp_path / "config.txt"
    source.write_text("Original", encoding="utf-8")
    config = {
        "lowercase": True,
        "chars_to_remove": ["x"],
        "nested": {"path": source},
    }

    result = export_processed_corpus(
        [
            ProcessedDocumentRecord.from_extracted_document(
                plain_text_extractor().extract(source), "processed"
            )
        ],
        config,
        tmp_path / "export",
    )

    expected_config = {
        "lowercase": True,
        "chars_to_remove": ["x"],
        "nested": {"path": str(source)},
    }
    assert read_json(result.config_path) == expected_config
    assert read_json(result.manifest_path)["processing_config"] == expected_config


def test_processed_text_hashes_are_recorded(tmp_path):
    source = tmp_path / "hash.txt"
    source.write_text("Original", encoding="utf-8")
    processed_text = "Processed hash text"

    result = export_processed_corpus(
        [
            ProcessedDocumentRecord.from_extracted_document(
                plain_text_extractor().extract(source), processed_text
            )
        ],
        {},
        tmp_path / "export",
    )
    _, document = single_document_manifest(result.output_directory)

    assert document["processed_text_hash"] == {
        "algorithm": "sha256",
        "value": hashlib.sha256(processed_text.encode("utf-8")).hexdigest(),
    }
    assert document["source_hash"] == {
        "algorithm": "sha256",
        "value": hashlib.sha256(b"Original").hexdigest(),
    }
