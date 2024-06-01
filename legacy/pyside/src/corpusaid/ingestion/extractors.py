"""Format-specific document extraction.
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
    """
    service = _default_service()
    directory_path = _coerce_path(directory)
    candidate_paths = set()

    for path in directory_path.rglob("*"):
        if path.is_file() and service.supports(path):
            candidate_paths.add(str(path))
    return sorted(candidate_paths)

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
