from pathlib import Path
from zipfile import ZipFile

import pytest

from corpusaid.ingestion import DocxExtractor, HtmlExtractor, PdfExtractor


def require_html_extractor():
    corpus_preview = pytest.importorskip("corpus_preview")
    if not hasattr(corpus_preview, "extract_html_text"):
        pytest.skip("corpus_preview.extract_html_text is unavailable")
    return corpus_preview


def require_scan_directory():
    corpus_preview = pytest.importorskip("corpus_preview")
    if not hasattr(corpus_preview, "scan_directory"):
        pytest.skip("corpus_preview.scan_directory is unavailable")
    return corpus_preview


def require_docx_extractor():
    corpus_preview = pytest.importorskip("corpus_preview")
    if not hasattr(corpus_preview, "extract_docx_text"):
        pytest.skip("corpus_preview.extract_docx_text is unavailable")
    return corpus_preview


def require_pdf_extractor():
    corpus_preview = pytest.importorskip("corpus_preview")
    if not hasattr(corpus_preview, "extract_pdf_text"):
        pytest.skip("corpus_preview.extract_pdf_text is unavailable")
    return corpus_preview


def write_docx(path: Path, document_xml: str):
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


def write_minimal_pdf(path: Path, text: str):
    escaped = text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
    stream = f"BT /F1 24 Tf 72 720 Td ({escaped}) Tj ET\n" if escaped else ""
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        "/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        f"<< /Length {len(stream.encode('ascii'))} >>\nstream\n{stream}endstream",
    ]
    payload = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for index, body in enumerate(objects, start=1):
        offsets.append(len(payload))
        payload.extend(f"{index} 0 obj\n{body}\nendobj\n".encode("ascii"))
    xref_offset = len(payload)
    payload.extend(f"xref\n0 {len(offsets)}\n".encode("ascii"))
    payload.extend(b"0000000000 65535 f \n")
    for offset in offsets[1:]:
        payload.extend(f"{offset:010} 00000 n \n".encode("ascii"))
    payload.extend(
        (
            f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\n"
            f"startxref\n{xref_offset}\n%%EOF\n"
        ).encode("ascii")
    )
    path.write_bytes(bytes(payload))


@pytest.mark.native
def test_native_extract_html_text_function(tmp_path):
    corpus_preview = require_html_extractor()
    path = tmp_path / "native.html"
    path.write_text(
        "<html><body><h1>Native Heading</h1>"
        "<p>Native paragraph text.</p></body></html>",
        encoding="utf-8",
    )

    text = corpus_preview.extract_html_text(str(path))

    assert "Native Heading" in text
    assert "Native paragraph text" in text


@pytest.mark.native
def test_html_extractor_uses_installed_native_backend(tmp_path):
    require_html_extractor()
    path = tmp_path / "native.html"
    path.write_text(
        "<html><head><title>Native Title</title></head>"
        "<body><p>Native extractor text.</p></body></html>",
        encoding="utf-8",
    )

    document = HtmlExtractor().extract(Path(path))

    assert document.document_type == "html"
    assert document.extraction_method == "rust:corpus_preview.extract_html_text"
    assert "Native extractor text" in document.text
    assert document.metadata["title"] == "Native Title"


@pytest.mark.native
def test_native_extract_docx_text_function(tmp_path):
    corpus_preview = require_docx_extractor()
    path = tmp_path / "native.docx"
    write_docx(
        path,
        (
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
            "<w:body>"
            "<w:p><w:r><w:t>Native paragraph</w:t></w:r></w:p>"
            "<w:tbl><w:tr>"
            "<w:tc><w:p><w:r><w:t>Cell 1</w:t></w:r></w:p></w:tc>"
            "<w:tc><w:p><w:r><w:t>Cell 2</w:t></w:r></w:p></w:tc>"
            "</w:tr></w:tbl>"
            "</w:body></w:document>"
        ),
    )

    text = corpus_preview.extract_docx_text(str(path))

    assert "Native paragraph" in text
    assert "Cell 1 | Cell 2" in text


@pytest.mark.native
def test_docx_extractor_uses_installed_native_backend(tmp_path):
    require_docx_extractor()
    path = tmp_path / "native.docx"
    write_docx(
        path,
        (
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
            "<w:body><w:p><w:r><w:t>Native DOCX extractor text.</w:t></w:r></w:p></w:body>"
            "</w:document>"
        ),
    )

    document = DocxExtractor().extract(Path(path))

    assert document.document_type == "docx"
    assert document.extraction_method == "rust:corpus_preview.extract_docx_text"
    assert "Native DOCX extractor text." in document.text


@pytest.mark.native
def test_native_extract_pdf_text_function(tmp_path):
    corpus_preview = require_pdf_extractor()
    path = tmp_path / "native.pdf"
    write_minimal_pdf(path, "Native PDF text")

    text, metadata = corpus_preview.extract_pdf_text(str(path))

    assert "Native PDF text" in text
    assert metadata["backend"] == "pdf-extract"
    assert metadata["native_backend"] is True
    assert metadata["page_count"] == 1


@pytest.mark.native
def test_pdf_extractor_uses_installed_native_backend(tmp_path):
    require_pdf_extractor()
    path = tmp_path / "native.pdf"
    write_minimal_pdf(path, "Native PDF extractor text.")

    document = PdfExtractor().extract(Path(path))

    assert document.document_type == "pdf"
    assert document.extraction_method == "rust:corpus_preview.extract_pdf_text"
    assert "Native PDF extractor text." in document.text
    assert document.metadata["backend"] == "pdf-extract"
    assert document.metadata["page_count"] == 1
    assert document.metadata["support_status"] == "experimental"


@pytest.mark.native
def test_native_scan_directory_includes_html(tmp_path):
    corpus_preview = require_scan_directory()
    html_path = tmp_path / "scan.html"
    text_path = tmp_path / "scan.txt"
    docx_path = tmp_path / "scan.docx"
    pdf_path = tmp_path / "scan.pdf"
    html_path.write_text("<html><body>Scan HTML</body></html>", encoding="utf-8")
    text_path.write_text("Scan text", encoding="utf-8")
    write_docx(
        docx_path,
        (
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
            "<w:body><w:p><w:r><w:t>Scan DOCX</w:t></w:r></w:p></w:body></w:document>"
        ),
    )
    write_minimal_pdf(pdf_path, "Scan PDF")

    scanned = {Path(path).name for path in corpus_preview.scan_directory(str(tmp_path))}

    assert "scan.html" in scanned
    assert "scan.txt" in scanned
    assert "scan.docx" in scanned
    assert "scan.pdf" in scanned
