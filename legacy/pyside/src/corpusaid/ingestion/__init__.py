"""Document ingestion primitives for CorpusAid."""

from .extractors import (
    DocxExtractor,
    DocumentIngestionService,
    DocumentPreview,
    HtmlExtractor,
    PdfExtractor,
    PlainTextExtractor,
    UnsupportedDocumentTypeError,
    can_overwrite_with_extracted_text,
    create_default_ingestion_service,
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
from .rust_backend import RustTextBackend
from .tika_backend import TikaServerBackend

__all__ = [
    "DocumentExtractor",
    "DocxExtractor",
    "DocumentIngestionService",
    "DocumentPreview",
    "ExtractedDocument",
    "ExtractionBackendResult",
    "ExtractionFallbackPolicy",
    "ExtractionManifest",
    "ExtractionWarning",
    "HtmlExtractor",
    "PdfExtractor",
    "PlainTextExtractor",
    "RustTextBackend",
    "TikaServerBackend",
    "UnsupportedDocumentTypeError",
    "can_overwrite_with_extracted_text",
    "create_default_ingestion_service",
    "extract_document",
    "load_document_preview",
    "scan_supported_paths",
    "supported_document_formats",
    "supported_file_extensions",
]
