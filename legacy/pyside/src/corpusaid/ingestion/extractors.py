"""Format-specific document extraction.

Plain-text, HTML, DOCX, and experimental PDF extraction are implemented. Future extractors should
implement ``DocumentExtractor`` and be registered in
``create_default_ingestion_service``.
"""

from __future__ import annotations

from html.parser import HTMLParser
import logging
from dataclasses import dataclass, field
from pathlib import Path
import time
import xml.etree.ElementTree as ET
from typing import Any, Dict, Iterable, List, Mapping, Optional, Tuple
from zipfile import BadZipFile, ZipFile

from .models import (
    DocumentExtractor,
    ExtractedDocument,
    ExtractionBackendResult,
    ExtractionFallbackPolicy,
    ExtractionWarning,
    PathLike,
)
from .backends import extraction_quality, orchestrate_extraction
from .rust_backend import RustTextBackend
from .tika_backend import TikaServerBackend


class UnsupportedDocumentTypeError(ValueError):
    """Raised when no registered extractor supports a document path."""


@dataclass
class DocumentPreview:
    """Preview text and provenance for UI preview loading."""

    text: str
    truncated: bool
    document_type: str
    extraction_method: str
    warnings: List[ExtractionWarning] = field(default_factory=list)
    metadata: Dict[str, object] = field(default_factory=dict)


class PlainTextExtractor:
    """Extractor for UTF-8 compatible ``.txt`` documents."""

    document_type = "txt"
    support_level = "stable"
    supported_extensions = {".txt"}
    can_overwrite_with_extracted_text = True

    def __init__(self, rust_backend: Optional[RustTextBackend] = None):
        self.rust_backend = rust_backend or RustTextBackend.from_optional_import()

    def supports(self, path: PathLike) -> bool:
        return _coerce_path(path).suffix.lower() in self.supported_extensions

    def extract(self, path: PathLike) -> ExtractedDocument:
        source_path = _coerce_path(path)
        if not self.supports(source_path):
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )

        warnings: List[ExtractionWarning] = []
        text: Optional[str] = None
        method = "python:utf-8"

        if self.rust_backend.load_full_text is not None:
            try:
                text = self.rust_backend.load_full_text(str(source_path))
                method = "rust:corpus_preview.load_full_text"
            except Exception as exc:  # pragma: no cover - exercised via unit test
                warnings.append(
                    ExtractionWarning(
                        code="rust_full_text_fallback",
                        message=(
                            "Rust full-text loader failed; used Python fallback "
                            "instead"
                        ),
                        details=str(exc),
                    )
                )

        if text is None:
            text, python_method, decode_warnings = _read_utf8_text(source_path)
            method = python_method
            warnings.extend(decode_warnings)

        return ExtractedDocument(
            source_path=source_path,
            text=text,
            document_type=self.document_type,
            extraction_method=method,
            warnings=warnings,
            metadata=_metadata_with_quality(
                text, _text_metadata(source_path), self.document_type, self.support_level
            ),
        )

    def preview(self, path: PathLike, limit: int) -> DocumentPreview:
        source_path = _coerce_path(path)
        if not self.supports(source_path):
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )

        warnings: List[ExtractionWarning] = []
        if self.rust_backend.load_preview is not None and limit > 0:
            try:
                preview_text, truncated = self.rust_backend.load_preview(
                    str(source_path), limit
                )
                return DocumentPreview(
                    text=preview_text,
                    truncated=truncated,
                    document_type=self.document_type,
                    extraction_method="rust:corpus_preview.load_preview",
                    warnings=warnings,
                    metadata=_text_metadata(source_path),
                )
            except Exception as exc:  # pragma: no cover - rust failure path
                warnings.append(
                    ExtractionWarning(
                        code="rust_preview_fallback",
                        message=(
                            "Rust preview loader failed; used Python fallback "
                            "instead"
                        ),
                        details=str(exc),
                    )
                )

        preview_text, truncated, method, decode_warnings = _read_utf8_preview(
            source_path, limit
        )
        warnings.extend(decode_warnings)
        return DocumentPreview(
            text=preview_text,
            truncated=truncated,
            document_type=self.document_type,
            extraction_method=method,
            warnings=warnings,
            metadata=_text_metadata(source_path),
        )


class HtmlExtractor:
    """Extractor for HTML documents."""

    document_type = "html"
    support_level = "stable"
    supported_extensions = {".html", ".htm"}
    can_overwrite_with_extracted_text = False

    def __init__(self, rust_backend: Optional[RustTextBackend] = None):
        self.rust_backend = rust_backend or RustTextBackend.from_optional_import()

    def supports(self, path: PathLike) -> bool:
        return _coerce_path(path).suffix.lower() in self.supported_extensions

    def extract(self, path: PathLike) -> ExtractedDocument:
        source_path = _coerce_path(path)
        if not self.supports(source_path):
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )

        html_source, _decode_method, decode_warnings = _read_utf8_text(source_path)
        metadata = _html_metadata(source_path, html_source)
        warnings = list(decode_warnings)
        text: Optional[str] = None
        method = "python:html.parser"

        if self.rust_backend.extract_html_text is not None:
            try:
                text = self.rust_backend.extract_html_text(str(source_path))
                method = "rust:corpus_preview.extract_html_text"
            except Exception as exc:  # pragma: no cover - exercised via unit test
                warnings.append(
                    ExtractionWarning(
                        code="rust_html_fallback",
                        message=(
                            "Rust HTML extractor failed; used Python fallback "
                            "instead"
                        ),
                        details=str(exc),
                    )
                )
        else:
            warnings.append(
                ExtractionWarning(
                    code="rust_html_unavailable",
                    message=(
                        "Rust HTML extractor unavailable; used Python fallback "
                        "instead"
                    ),
                )
            )

        if text is None:
            text, parser_warnings = _extract_html_with_python(html_source)
            warnings.extend(parser_warnings)

        if not text.strip():
            warnings.append(
                ExtractionWarning(
                    code="empty_html_extraction",
                    message="HTML extraction produced no text",
                )
            )

        return ExtractedDocument(
            source_path=source_path,
            text=text,
            document_type=self.document_type,
            extraction_method=method,
            warnings=warnings,
            metadata=_metadata_with_quality(
                text, metadata, self.document_type, self.support_level
            ),
        )


class DocxExtractor:
    """Extractor for Office Open XML ``.docx`` documents."""

    document_type = "docx"
    support_level = "experimental"
    supported_extensions = {".docx"}
    can_overwrite_with_extracted_text = False

    def __init__(self, rust_backend: Optional[RustTextBackend] = None):
        self.rust_backend = rust_backend or RustTextBackend.from_optional_import()

    def supports(self, path: PathLike) -> bool:
        return _coerce_path(path).suffix.lower() in self.supported_extensions

    def extract(self, path: PathLike) -> ExtractedDocument:
        source_path = _coerce_path(path)
        if not self.supports(source_path):
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )

        metadata, package_warnings = _docx_package_metadata(source_path)
        warnings = list(package_warnings)
        text: Optional[str] = None
        method = "python:zip+xml"

        if self.rust_backend.extract_docx_text is not None:
            try:
                text = self.rust_backend.extract_docx_text(str(source_path))
                method = "rust:corpus_preview.extract_docx_text"
            except Exception as exc:  # pragma: no cover - exercised via unit test
                warnings.append(
                    ExtractionWarning(
                        code="rust_docx_fallback",
                        message=(
                            "Rust DOCX extractor failed; used Python fallback "
                            "instead"
                        ),
                        details=str(exc),
                    )
                )
        else:
            warnings.append(
                ExtractionWarning(
                    code="rust_docx_unavailable",
                    message=(
                        "Rust DOCX extractor unavailable; used Python fallback "
                        "instead"
                    ),
                )
            )

        if text is None:
            text, parser_warnings = _extract_docx_with_python(source_path)
            warnings.extend(parser_warnings)

        if not text.strip():
            warnings.append(
                ExtractionWarning(
                    code="empty_docx_extraction",
                    message="DOCX extraction produced no text",
                )
            )

        return ExtractedDocument(
            source_path=source_path,
            text=text,
            document_type=self.document_type,
            extraction_method=method,
            warnings=warnings,
            metadata=_metadata_with_quality(
                text, metadata, self.document_type, self.support_level
            ),
        )


_ENV_CONFIGURED_FALLBACK = object()


class PdfExtractor:
    """Experimental extractor for born-digital PDFs with embedded text."""

    document_type = "pdf"
    support_level = "experimental"
    supported_extensions = {".pdf"}
    can_overwrite_with_extracted_text = False
    backend_name = "pdf-extract"

    def __init__(
        self,
        rust_backend: Optional[RustTextBackend] = None,
        fallback_backend: object = _ENV_CONFIGURED_FALLBACK,
        fallback_policy: Optional[ExtractionFallbackPolicy] = None,
    ):
        self.rust_backend = rust_backend or RustTextBackend.from_optional_import()
        self.fallback_backend = (
            TikaServerBackend.from_environment()
            if fallback_backend is _ENV_CONFIGURED_FALLBACK
            else fallback_backend
        )
        self.fallback_policy = fallback_policy or ExtractionFallbackPolicy()

    def supports(self, path: PathLike) -> bool:
        return _coerce_path(path).suffix.lower() in self.supported_extensions

    def extract(self, path: PathLike) -> ExtractedDocument:
        source_path = _coerce_path(path)
        if not self.supports(source_path):
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )

        source_path.stat()
        metadata = _pdf_metadata(source_path, self.backend_name)
        primary = self._extract_with_rust(source_path, metadata)

        fallback_factory = None
        if self.fallback_backend is not None:
            fallback_factory = lambda reason: self.fallback_backend.extract(
                source_path, fallback_reason=reason
            )

        orchestration = orchestrate_extraction(
            primary=primary,
            fallback_factory=fallback_factory,
            policy=self.fallback_policy,
            document_type=self.document_type,
            support_status=self.support_level,
            base_metadata=metadata,
        )

        return ExtractedDocument(
            source_path=source_path,
            text=orchestration.text,
            document_type=self.document_type,
            extraction_method=orchestration.extraction_method,
            warnings=orchestration.warnings,
            metadata=orchestration.metadata,
        )

    def _extract_with_rust(
        self,
        source_path: Path,
        metadata: Mapping[str, object],
    ) -> ExtractionBackendResult:
        method = "rust:corpus_preview.extract_pdf_text"
        start = time.perf_counter()

        if self.rust_backend.extract_pdf_text is None:
            return ExtractionBackendResult(
                backend_name="unavailable:corpus_preview.extract_pdf_text",
                success=False,
                text="",
                warnings=[
                    ExtractionWarning(
                        code="rust_pdf_unavailable",
                        message=(
                            "Rust PDF extractor unavailable; no PDF fallback is "
                            "configured"
                        ),
                    )
                ],
                metadata=dict(metadata),
                elapsed_seconds=time.perf_counter() - start,
                support_status=self.support_level,
                error="corpus_preview.extract_pdf_text is unavailable",
            )

        try:
            result = self.rust_backend.extract_pdf_text(str(source_path))
            text, native_metadata = _coerce_pdf_backend_result(result)
            result_metadata = dict(metadata)
            result_metadata.update(native_metadata)
            result_metadata["backend"] = result_metadata.get("backend") or self.backend_name
            result_metadata["native_backend"] = bool(
                result_metadata.get("native_backend", True)
            )
            return ExtractionBackendResult(
                backend_name=method,
                success=True,
                text=text,
                warnings=[],
                metadata=result_metadata,
                elapsed_seconds=time.perf_counter() - start,
                support_status=self.support_level,
            )
        except Exception as exc:  # pragma: no cover - exercised via unit test
            warning = (
                ExtractionWarning(
                    code="encrypted_pdf_unsupported",
                    message=(
                        "Encrypted or password-protected PDF could not be "
                        "extracted"
                    ),
                    details=str(exc),
                )
                if _pdf_error_looks_encrypted(exc)
                else ExtractionWarning(
                    code="pdf_extraction_failed",
                    message="PDF extraction failed",
                    details=str(exc),
                )
            )
            result_metadata = dict(metadata)
            result_metadata["native_backend"] = False
            return ExtractionBackendResult(
                backend_name=method,
                success=False,
                text="",
                warnings=[warning],
                metadata=result_metadata,
                elapsed_seconds=time.perf_counter() - start,
                support_status=self.support_level,
                error=str(exc),
            )


class DocumentIngestionService:
    """Registry that selects the first extractor supporting a path."""

    def __init__(self, extractors: Iterable[DocumentExtractor]):
        self.extractors = list(extractors)

    def supports(self, path: PathLike) -> bool:
        return self._find_extractor(path) is not None

    def extract(self, path: PathLike) -> ExtractedDocument:
        extractor = self._find_extractor(path)
        if extractor is None:
            source_path = _coerce_path(path)
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )
        return extractor.extract(path)

    def preview(self, path: PathLike, limit: int) -> DocumentPreview:
        extractor = self._find_extractor(path)
        if extractor is None:
            source_path = _coerce_path(path)
            raise UnsupportedDocumentTypeError(
                f"Unsupported document type: {source_path.suffix or '<none>'}"
            )
        preview = getattr(extractor, "preview", None)
        if preview is None:
            document = extractor.extract(path)
            text, truncated = _truncate(document.text, limit)
            return DocumentPreview(
                text=text,
                truncated=truncated,
                document_type=document.document_type,
                extraction_method=document.extraction_method,
                warnings=list(document.warnings),
                metadata=dict(document.metadata),
            )
        return preview(path, limit)

    def _find_extractor(self, path: PathLike) -> Optional[DocumentExtractor]:
        for extractor in self.extractors:
            if extractor.supports(path):
                return extractor
        return None

    def supported_extensions(self) -> List[str]:
        extensions = set()
        for extractor in self.extractors:
            extensions.update(
                _normalized_extensions(
                    getattr(extractor, "supported_extensions", set())
                )
            )
        return sorted(extensions)

    def supported_document_formats(self) -> List[Dict[str, object]]:
        formats = []
        for extractor in self.extractors:
            extensions = _normalized_extensions(
                getattr(extractor, "supported_extensions", set())
            )
            if not extensions:
                continue
            formats.append(
                {
                    "document_type": getattr(
                        extractor, "document_type", extractor.__class__.__name__
                    ),
                    "extensions": extensions,
                    "support_level": getattr(extractor, "support_level", "unknown"),
                    "can_overwrite_with_extracted_text": bool(
                        getattr(extractor, "can_overwrite_with_extracted_text", False)
                    ),
                }
            )
        return sorted(formats, key=lambda item: str(item["document_type"]))

    def can_overwrite_with_extracted_text(self, path: PathLike) -> bool:
        extractor = self._find_extractor(path)
        if extractor is None:
            return False
        return bool(getattr(extractor, "can_overwrite_with_extracted_text", False))


def create_default_ingestion_service(
    rust_backend: Optional[RustTextBackend] = None,
) -> DocumentIngestionService:
    backend = rust_backend or RustTextBackend.from_optional_import()
    return DocumentIngestionService(
        [
            PlainTextExtractor(backend),
            HtmlExtractor(backend),
            DocxExtractor(backend),
            PdfExtractor(backend),
        ]
    )


_DEFAULT_SERVICE: Optional[DocumentIngestionService] = None


def _default_service() -> DocumentIngestionService:
    global _DEFAULT_SERVICE
    if _DEFAULT_SERVICE is None:
        _DEFAULT_SERVICE = create_default_ingestion_service()
    return _DEFAULT_SERVICE


def extract_document(path: PathLike) -> ExtractedDocument:
    return _default_service().extract(path)


def load_document_preview(path: PathLike, limit: int) -> DocumentPreview:
    return _default_service().preview(path, limit)


def supported_file_extensions() -> List[str]:
    return _default_service().supported_extensions()


def supported_document_formats() -> List[Dict[str, object]]:
    return _default_service().supported_document_formats()


def can_overwrite_with_extracted_text(path: PathLike) -> bool:
    return _default_service().can_overwrite_with_extracted_text(path)


def scan_supported_paths(directory: PathLike) -> List[str]:
    """Return supported document paths under a directory.

    The optional Rust scanner is used when available, then filtered through the
    registered extractor set. The Python walker remains the fallback and uses
    the same ``supports`` checks.
    """

    service = _default_service()
    directory_path = _coerce_path(directory)
    rust_backend = _rust_backend_from_service(service)

    candidate_paths = set()

    if rust_backend and rust_backend.scan_directory is not None:
        try:
            candidate_paths.update(
                str(_coerce_path(path))
                for path in rust_backend.scan_directory(str(directory_path))
                if service.supports(path)
            )
        except Exception as exc:
            logging.warning(
                "Rust directory scan failed for %s: %s. Falling back to Python walker.",
                directory_path,
                exc,
            )

    for path in directory_path.rglob("*"):
        if path.is_file() and service.supports(path):
            candidate_paths.add(str(path))
    return sorted(candidate_paths)


def _rust_backend_from_service(
    service: DocumentIngestionService,
) -> Optional[RustTextBackend]:
    for extractor in service.extractors:
        rust_backend = getattr(extractor, "rust_backend", None)
        if isinstance(rust_backend, RustTextBackend):
            return rust_backend
    return None


def _coerce_path(path: PathLike) -> Path:
    return Path(path)


def _normalized_extensions(extensions: Iterable[object]) -> List[str]:
    normalized = set()
    for extension in extensions:
        extension = str(extension).strip().lower()
        if not extension:
            continue
        if not extension.startswith("."):
            extension = f".{extension}"
        normalized.add(extension)
    return sorted(normalized)


def _read_utf8_text(path: Path) -> Tuple[str, str, List[ExtractionWarning]]:
    try:
        return path.read_text(encoding="utf-8"), "python:utf-8", []
    except UnicodeDecodeError as exc:
        text = path.read_text(encoding="utf-8", errors="replace")
        return (
            text,
            "python:utf-8-replace",
            [
                ExtractionWarning(
                    code="invalid_utf8_replacement",
                    message="Invalid UTF-8 bytes were replaced while reading text",
                    details=str(exc),
                )
            ],
        )


def _read_utf8_preview(
    path: Path,
    limit: int,
) -> Tuple[str, bool, str, List[ExtractionWarning]]:
    if limit is None or limit <= 0:
        text, method, warnings = _read_utf8_text(path)
        return text, False, method, warnings

    try:
        with path.open("r", encoding="utf-8") as file:
            preview = file.read(limit + 1)
        method = "python:utf-8"
        warnings: List[ExtractionWarning] = []
    except UnicodeDecodeError as exc:
        with path.open("r", encoding="utf-8", errors="replace") as file:
            preview = file.read(limit + 1)
        method = "python:utf-8-replace"
        warnings = [
            ExtractionWarning(
                code="invalid_utf8_preview_replacement",
                message="Invalid UTF-8 bytes were replaced while reading preview",
                details=str(exc),
            )
        ]

    truncated = len(preview) > limit
    return preview[:limit], truncated, method, warnings


def _truncate(text: str, limit: int) -> Tuple[str, bool]:
    if limit is None or limit <= 0:
        return text, False
    if len(text) > limit:
        return text[:limit], True
    return text, False


def _text_metadata(path: Path) -> Dict[str, object]:
    metadata = {"extension": path.suffix.lower()}
    try:
        metadata["size_bytes"] = path.stat().st_size
    except OSError:
        pass
    return metadata


def _metadata_with_quality(
    text: str,
    metadata: Dict[str, object],
    document_type: str,
    support_level: str,
) -> Dict[str, object]:
    quality_metadata, _warnings = extraction_quality(
        text,
        document_type=document_type,
        metadata=metadata,
        support_status=support_level,
    )
    metadata.update(quality_metadata)
    return metadata


def _pdf_metadata(path: Path, backend: str) -> Dict[str, object]:
    metadata = _text_metadata(path)
    metadata.update(
        {
            "backend": backend,
            "native_backend": False,
            "source_type": "pdf",
            "support_status": "experimental",
        }
    )
    return metadata


def _coerce_pdf_backend_result(result: Any) -> Tuple[str, Dict[str, object]]:
    if isinstance(result, tuple):
        text = "" if not result else str(result[0])
        metadata = _mapping_to_metadata(result[1]) if len(result) > 1 else {}
        return text, metadata

    if isinstance(result, Mapping):
        result_mapping = _mapping_to_metadata(result)
        text = str(result_mapping.pop("text", ""))
        metadata_value = result_mapping.pop("metadata", None)
        metadata = _mapping_to_metadata(metadata_value)
        metadata.update(result_mapping)
        return text, metadata

    return str(result), {}


def _mapping_to_metadata(value: Any) -> Dict[str, object]:
    if not isinstance(value, Mapping):
        return {}
    return {str(key): _metadata_scalar(item) for key, item in value.items()}


def _metadata_scalar(value: Any) -> object:
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, Mapping):
        return _mapping_to_metadata(value)
    if isinstance(value, (list, tuple, set)):
        return [_metadata_scalar(item) for item in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


def _pdf_error_looks_encrypted(exc: Exception) -> bool:
    message = str(exc).lower()
    return any(token in message for token in ("encrypt", "password", "decrypt"))


class _HtmlTextParser(HTMLParser):
    block_tags = {
        "address",
        "article",
        "aside",
        "blockquote",
        "br",
        "dd",
        "div",
        "dl",
        "dt",
        "figcaption",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hr",
        "li",
        "main",
        "nav",
        "ol",
        "p",
        "pre",
        "section",
        "table",
        "td",
        "th",
        "tr",
        "ul",
    }
    skip_tags = {"script", "style", "noscript"}
    void_tags = {
        "area",
        "base",
        "br",
        "col",
        "embed",
        "hr",
        "img",
        "input",
        "link",
        "meta",
        "param",
        "source",
        "track",
        "wbr",
    }

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts: List[str] = []
        self.title_parts: List[str] = []
        self.open_tags: List[str] = []
        self.skip_depth = 0
        self.in_title = False
        self.malformed = False

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        if tag in self.skip_tags:
            self.skip_depth += 1
            return
        if self.skip_depth:
            return
        if tag == "title":
            self.in_title = True
            self.open_tags.append(tag)
            return
        if tag in self.block_tags:
            self._append_boundary()
        if tag not in self.void_tags:
            self.open_tags.append(tag)

    def handle_startendtag(self, tag, attrs):
        tag = tag.lower()
        if tag in self.skip_tags:
            return
        if self.skip_depth:
            return
        if tag in self.block_tags:
            self._append_boundary()

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag in self.skip_tags:
            self.skip_depth = max(0, self.skip_depth - 1)
            return
        if self.skip_depth:
            return
        if tag == "title":
            self.in_title = False
        if tag in self.open_tags:
            while self.open_tags:
                current = self.open_tags.pop()
                if current == tag:
                    break
                self.malformed = True
        else:
            self.malformed = True
        if tag in self.block_tags:
            self._append_boundary()

    def handle_data(self, data):
        if self.skip_depth:
            return
        if self.in_title:
            self.title_parts.append(data)
            return
        self.parts.append(data)

    def close(self):
        super().close()
        if self.open_tags:
            self.malformed = True

    def _append_boundary(self):
        if self.parts and self.parts[-1] != "\n":
            self.parts.append("\n")


def _extract_html_with_python(html_source: str) -> Tuple[str, List[ExtractionWarning]]:
    parser = _HtmlTextParser()
    parser.feed(html_source)
    parser.close()

    warnings = []
    if parser.malformed:
        warnings.append(
            ExtractionWarning(
                code="malformed_html_recovered",
                message="Malformed HTML was recovered by the Python parser",
            )
        )

    return _normalize_html_text(parser.parts), warnings


def _html_metadata(path: Path, html_source: str) -> Dict[str, object]:
    metadata = _text_metadata(path)
    parser = _HtmlTextParser()
    parser.feed(html_source)
    parser.close()
    title = _normalize_inline_text(" ".join(parser.title_parts))
    if title:
        metadata["title"] = title
    return metadata


def _normalize_html_text(parts: List[str]) -> str:
    raw = "".join(parts)
    lines = [_normalize_inline_text(line) for line in raw.splitlines()]
    return "\n".join(line for line in lines if line).strip()


def _normalize_inline_text(text: str) -> str:
    return " ".join(text.split())


_WORD_NAMESPACE = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
_TRACKED_CHANGE_TAGS = {"ins", "del", "moveFrom", "moveTo"}


def _extract_docx_with_python(path: Path) -> Tuple[str, List[ExtractionWarning]]:
    try:
        with ZipFile(path) as archive:
            try:
                document_xml = archive.read("word/document.xml")
            except KeyError as exc:
                raise ValueError("Invalid DOCX file: missing word/document.xml") from exc
    except BadZipFile as exc:
        raise ValueError(f"Invalid DOCX file: {exc}") from exc

    try:
        root = ET.fromstring(document_xml)
    except ET.ParseError as exc:
        raise ValueError(f"Invalid DOCX XML in word/document.xml: {exc}") from exc

    warnings: List[ExtractionWarning] = []
    if _docx_has_tracked_changes(root):
        warnings.append(
            ExtractionWarning(
                code="docx_tracked_changes_not_extracted",
                message="Tracked changes were detected but not extracted separately",
            )
        )

    paragraphs = []
    body = root.find("w:body", _WORD_NAMESPACE)
    if body is None:
        return "", warnings

    for child in body:
        tag = _local_name(child.tag)
        if tag == "p":
            paragraph = _docx_paragraph_text(child)
            if paragraph:
                paragraphs.append(paragraph)
        elif tag == "tbl":
            paragraphs.extend(_docx_table_lines(child))

    return "\n".join(paragraphs).strip(), warnings


def _docx_package_metadata(path: Path) -> Tuple[Dict[str, object], List[ExtractionWarning]]:
    metadata = _text_metadata(path)
    warnings: List[ExtractionWarning] = []
    try:
        with ZipFile(path) as archive:
            names = set(archive.namelist())
    except BadZipFile as exc:
        raise ValueError(f"Invalid DOCX file: {exc}") from exc

    extra_parts = []
    if "word/comments.xml" in names:
        extra_parts.append("comments")
    if "word/footnotes.xml" in names:
        extra_parts.append("footnotes")
    if "word/endnotes.xml" in names:
        extra_parts.append("endnotes")
    if any(name.startswith("word/header") and name.endswith(".xml") for name in names):
        extra_parts.append("headers")
    if any(name.startswith("word/footer") and name.endswith(".xml") for name in names):
        extra_parts.append("footers")

    if extra_parts:
        metadata["unextracted_parts"] = extra_parts
        warnings.append(
            ExtractionWarning(
                code="docx_unextracted_parts",
                message="DOCX contains parts that are not extracted by current DOCX support",
                details=", ".join(extra_parts),
            )
        )

    metadata["has_tables"] = "word/document.xml" in names and _docx_archive_has_tag(
        path, "tbl"
    )
    return metadata, warnings


def _docx_archive_has_tag(path: Path, local_tag: str) -> bool:
    try:
        with ZipFile(path) as archive:
            document_xml = archive.read("word/document.xml")
    except (BadZipFile, KeyError):
        return False

    try:
        root = ET.fromstring(document_xml)
    except ET.ParseError:
        return False

    return root.find(f".//w:{local_tag}", _WORD_NAMESPACE) is not None


def _docx_has_tracked_changes(root: ET.Element) -> bool:
    for element in root.iter():
        if _local_name(element.tag) in _TRACKED_CHANGE_TAGS:
            return True
    return False


def _docx_table_lines(table: ET.Element) -> List[str]:
    lines = []
    for row in table.findall("w:tr", _WORD_NAMESPACE):
        cells = []
        for cell in row.findall("w:tc", _WORD_NAMESPACE):
            cell_parts = []
            for paragraph in cell.findall("w:p", _WORD_NAMESPACE):
                paragraph_text = _docx_paragraph_text(paragraph)
                if paragraph_text:
                    cell_parts.append(paragraph_text)
            cell_text = "\n".join(cell_parts).strip()
            if cell_text:
                cells.append(cell_text)
        if cells:
            lines.append(" | ".join(cells))
    return lines


def _docx_paragraph_text(paragraph: ET.Element) -> str:
    parts: List[str] = []
    for element in paragraph.iter():
        tag = _local_name(element.tag)
        if tag == "t" and element.text:
            parts.append(element.text)
        elif tag in {"tab"}:
            parts.append("\t")
        elif tag in {"br", "cr"}:
            parts.append("\n")
    return _normalize_docx_paragraph("".join(parts))


def _normalize_docx_paragraph(text: str) -> str:
    lines = [" ".join(line.split()) for line in text.splitlines()]
    cleaned = "\n".join(line for line in lines if line)
    return cleaned.strip()


def _local_name(tag: str) -> str:
    if "}" in tag:
        return tag.rsplit("}", 1)[1]
    return tag
