"""Core document ingestion models.

This module is intentionally free of GUI and NLP dependencies so extraction can
grow independently from the PySide orchestration layer.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
from pathlib import Path
from typing import Any, Dict, List, Optional, Protocol, Union

PathLike = Union[str, Path]


@dataclass(frozen=True)
class ExtractionWarning:
    """Structured warning emitted while extracting a document."""

    code: str
    message: str
    details: Optional[str] = None

    def __str__(self) -> str:
        if self.details:
            return f"{self.message} ({self.details})"
        return self.message

    def to_json_dict(self) -> Dict[str, Optional[str]]:
        """Return a JSON-compatible representation."""
        return {
            "code": self.code,
            "message": self.message,
            "details": self.details,
        }


@dataclass
class ExtractedDocument:
    """Text and provenance produced by a document extractor."""

    source_path: Path
    text: str
    document_type: str
    extraction_method: str
    warnings: List[ExtractionWarning] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)


@dataclass
class ExtractionBackendResult:
    """Result from one extraction backend attempt."""

    backend_name: str
    success: bool
    text: str = ""
    warnings: List[ExtractionWarning] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    elapsed_seconds: Optional[float] = None
    support_status: Optional[str] = None
    error: Optional[str] = None

    def to_json_dict(self) -> Dict[str, Any]:
        """Return a JSON-compatible representation for manifests."""
        return {
            "backend_name": self.backend_name,
            "success": self.success,
            "elapsed_seconds": self.elapsed_seconds,
            "support_status": self.support_status,
            "error": self.error,
            "extracted_character_count": len(self.text),
            "warnings": [warning.to_json_dict() for warning in self.warnings],
            "metadata": _json_compatible(self.metadata),
        }


@dataclass(frozen=True)
class ExtractionFallbackPolicy:
    """Policy controlling when a fallback backend may be attempted."""

    enabled: bool = True
    fallback_on_failure: bool = True
    fallback_on_empty: bool = True
    fallback_on_near_empty: bool = True
    fallback_on_low_yield: bool = True
    near_empty_character_threshold: int = 40
    low_yield_min_source_bytes: int = 10_000
    low_yield_ratio: float = 0.005


@dataclass
class ExtractionManifest:
    """Small JSON-serialisable provenance record for extracted text."""

    source_path: Path
    document_type: str
    extraction_method: str
    extracted_character_count: int
    warnings: List[ExtractionWarning] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    app_version: Optional[str] = None
    project_version: Optional[str] = None
    source_file_hash: Optional[Dict[str, str]] = None

    @classmethod
    def from_document(
        cls,
        document: ExtractedDocument,
        *,
        app_version: Optional[str] = None,
        project_version: Optional[str] = None,
        include_source_hash: bool = False,
    ) -> "ExtractionManifest":
        source_file_hash = None
        if include_source_hash:
            source_file_hash = _hash_source_file(document.source_path)

        return cls(
            source_path=document.source_path,
            document_type=document.document_type,
            extraction_method=document.extraction_method,
            extracted_character_count=len(document.text),
            warnings=list(document.warnings),
            metadata=dict(document.metadata),
            app_version=app_version,
            project_version=project_version,
            source_file_hash=source_file_hash,
        )

    def to_json_dict(self) -> Dict[str, Any]:
        """Return a dictionary that can be passed directly to ``json.dumps``."""
        return {
            "app_version": self.app_version,
            "project_version": self.project_version,
            "source_path": str(self.source_path),
            "document_type": self.document_type,
            "extraction_method": self.extraction_method,
            "warnings": [warning.to_json_dict() for warning in self.warnings],
            "metadata": _json_compatible(self.metadata),
            "extracted_character_count": self.extracted_character_count,
            "source_file_hash": self.source_file_hash,
        }


class DocumentExtractor(Protocol):
    """Minimal protocol implemented by format-specific extractors."""

    def supports(self, path: PathLike) -> bool:
        """Return whether this extractor can handle the path."""
        ...

    def extract(self, path: PathLike) -> ExtractedDocument:
        """Extract text and metadata from the path."""
        ...


def _hash_source_file(path: Path) -> Optional[Dict[str, str]]:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as file:
            for chunk in iter(lambda: file.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError:
        return None

    return {
        "algorithm": "sha256",
        "value": digest.hexdigest(),
    }


def _json_compatible(value: Any) -> Any:
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, dict):
        return {str(key): _json_compatible(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_compatible(item) for item in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)
