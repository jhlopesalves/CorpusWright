"""Document ingestion primitives for CorpusAid."""

from .extractors import (
    DocumentIngestionService,
    DocumentPreview,
    HtmlExtractor,
    PlainTextExtractor,
    UnsupportedDocumentTypeError,
    can_overwrite_with_extracted_text,
    extract_document,
    load_document_preview,
    scan_supported_paths,
    supported_document_formats,
    supported_file_extensions,
)
from .models import (
    DocumentExtractor,
    ExtractedDocument,
    ExtractionBackendResult,
    ExtractionFallbackPolicy,
    ExtractionManifest,
    ExtractionWarning,
)

__all__ = [
    "DocumentExtractor",
    "DocumentIngestionService",
    "DocumentPreview",
    "ExtractedDocument",
    "ExtractionBackendResult",
    "ExtractionFallbackPolicy",
    "ExtractionManifest",
    "ExtractionWarning",
    "HtmlExtractor",
    "PlainTextExtractor",
    "UnsupportedDocumentTypeError",
    "can_overwrite_with_extracted_text",
    "extract_document",
    "load_document_preview",
    "scan_supported_paths",
    "supported_document_formats",
    "supported_file_extensions",
]
