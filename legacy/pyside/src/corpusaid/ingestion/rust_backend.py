"""Optional Rust backend adapters for document ingestion.

The GUI should not import Rust helpers directly. Future native extractors can add
new callables here and keep format-specific fallback behavior in extractor code.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Optional, Sequence, Tuple

ExtractDocxText = Callable[[str], str]
ExtractHtmlText = Callable[[str], str]
ExtractPdfText = Callable[[str], Any]
LoadFullText = Callable[[str], str]
LoadPreview = Callable[[str, int], Tuple[str, bool]]
ScanDirectory = Callable[[str], Sequence[str]]


@dataclass
class RustTextBackend:
    """Optional Rust text helpers exposed by the ``corpus_preview`` extension."""

    load_full_text: Optional[LoadFullText] = None
    load_preview: Optional[LoadPreview] = None
    scan_directory: Optional[ScanDirectory] = None
    extract_html_text: Optional[ExtractHtmlText] = None
    extract_docx_text: Optional[ExtractDocxText] = None
    extract_pdf_text: Optional[ExtractPdfText] = None

    @classmethod
    def from_optional_import(cls) -> "RustTextBackend":
        try:
            import corpus_preview  # type: ignore[import-not-found]
        except ImportError:
            return cls()
        return cls(
            load_full_text=getattr(corpus_preview, "load_full_text", None),
            load_preview=getattr(corpus_preview, "load_preview", None),
            scan_directory=getattr(corpus_preview, "scan_directory", None),
            extract_html_text=getattr(corpus_preview, "extract_html_text", None),
            extract_docx_text=getattr(corpus_preview, "extract_docx_text", None),
            extract_pdf_text=getattr(corpus_preview, "extract_pdf_text", None),
        )
