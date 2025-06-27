"""GUI/ingestion boundary tests.

These tests import the PySide entry-point module but do not launch the GUI.
They skip when PySide6 or Qt WebEngine is unavailable locally.
"""

from pathlib import Path

import pytest

pytestmark = pytest.mark.integration


@pytest.fixture(scope="module")
def corpusaid_module():
    pytest.importorskip("PySide6")
    return pytest.importorskip("CorpusAid")


def test_gui_file_filter_uses_registered_ingestion_extensions(corpusaid_module):
    assert (
        corpusaid_module.supported_file_dialog_filter()
        == "Supported Documents (*.docx *.htm *.html *.pdf *.txt)"
    )


def test_drag_drop_helper_accepts_only_supported_files(tmp_path, corpusaid_module):
    text_path = tmp_path / "sample.txt"
    html_path = tmp_path / "sample.HTML"
    pdf_path = tmp_path / "sample.PDF"
    unsupported_path = tmp_path / "sample.epub"
    missing_path = tmp_path / "missing.txt"
    for path in (text_path, html_path, pdf_path, unsupported_path):
        path.write_text("content", encoding="utf-8")

    assert corpusaid_module.is_supported_document_path(str(text_path)) is True
    assert corpusaid_module.is_supported_document_path(str(html_path)) is True
    assert corpusaid_module.is_supported_document_path(str(pdf_path)) is True
    assert corpusaid_module.is_supported_document_path(str(unsupported_path)) is False
    assert corpusaid_module.is_supported_document_path(str(missing_path)) is False


def test_structured_processed_export_path_uses_unique_txt_targets(
    tmp_path, corpusaid_module
):
    output_dir = tmp_path / "exports"
    output_dir.mkdir()
    existing = output_dir / "sample.txt"
    existing.write_text("existing", encoding="utf-8")
    reserved = set()

    first = corpusaid_module.processed_text_export_path(
        str(output_dir), str(Path("sample.docx")), reserved
    )
    second = corpusaid_module.processed_text_export_path(
        str(output_dir), str(Path("sample.html")), reserved
    )

    assert Path(first).name == "sample-2.txt"
    assert Path(second).name == "sample-3.txt"


def test_gui_processed_corpus_record_carries_ingestion_provenance(
    tmp_path, corpusaid_module
):
    source = tmp_path / "page.html"
    source.write_text(
        "<html><body><p>Original HTML text</p></body></html>",
        encoding="utf-8",
    )
    document = corpusaid_module.Document(str(source))

    document.update_processed_text("Processed HTML text")
    record = corpusaid_module.processed_corpus_record_from_document(document)
    result = corpusaid_module.write_processed_corpus_export(
        [record],
        {"lowercase": True},
        tmp_path / "export",
        app_version="test",
    )

    assert record.document_type == "html"
    assert record.extraction_method in {
        "python:html.parser",
        "rust:corpus_preview.extract_html_text",
    }
    assert record.original_text.strip() == "Original HTML text"
    assert record.processed_text == "Processed HTML text"
    assert result.files_exported == 1
    assert next(result.texts_directory.glob("*.txt")).read_text(
        encoding="utf-8"
    ) == "Processed HTML text"


def test_gui_processed_corpus_record_carries_pdf_provenance(
    tmp_path, corpusaid_module
):
    source = tmp_path / "paper.pdf"
    source.write_bytes(b"%PDF-1.4\n")
    document = corpusaid_module.Document(str(source))

    document.update_processed_text("Processed PDF text")
    record = corpusaid_module.processed_corpus_record_from_document(document)
    result = corpusaid_module.write_processed_corpus_export(
        [record],
        {},
        tmp_path / "export",
        app_version="test",
    )
    manifest_document = result.manifest["documents"][0]

    assert record.document_type == "pdf"
    assert record.extraction_method in {
        "unavailable:corpus_preview.extract_pdf_text",
        "rust:corpus_preview.extract_pdf_text",
    }
    assert record.processed_text == "Processed PDF text"
    warning_codes = [warning.code for warning in record.warnings]
    assert warning_codes[0] in {"rust_pdf_unavailable", "pdf_extraction_failed"}
    assert "fallback_not_configured" in warning_codes
    assert "empty_pdf_extraction" in warning_codes
    assert manifest_document["support_status"] == "experimental"
    assert manifest_document["structured_source"] is True
    assert manifest_document["metadata"]["fallback_attempted"] is False
    assert manifest_document["metadata"]["fallback_reason"] == "primary_failed"
