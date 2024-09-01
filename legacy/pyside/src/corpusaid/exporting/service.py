"""Safe processed-corpus export service.

This module is intentionally free of GUI and NLP dependencies. It writes
processed text as plain UTF-8 ``.txt`` files and records enough provenance for a
researcher to reproduce or audit the exported corpus later.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
import csv
import hashlib
import json
import re
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Optional, Sequence, Tuple, Union

from corpusaid.ingestion import ExtractedDocument, ExtractionWarning
from corpusaid.ingestion.extractors import supported_document_formats

HashPayload = Dict[str, str]
WarningLike = Union[ExtractionWarning, Mapping[str, Any], str]


@dataclass
class ProcessedDocumentRecord:
    """Processed text plus optional extraction provenance for one source file."""

    source_path: Union[str, Path]
    processed_text: str
    original_text: Optional[str] = None
    document_type: Optional[str] = None
    support_status: Optional[str] = None
    extraction_method: Optional[str] = None
    warnings: Sequence[WarningLike] = field(default_factory=list)
    metadata: Mapping[str, Any] = field(default_factory=dict)
    extracted_character_count: Optional[int] = None
    source_hash: Optional[HashPayload] = None
    structured_source: Optional[bool] = None

    @classmethod
    def from_extracted_document(
        cls,
        document: ExtractedDocument,
        processed_text: Optional[str] = None,
        *,
        support_status: Optional[str] = None,
        structured_source: Optional[bool] = None,
    ) -> "ProcessedDocumentRecord":
        """Create a processed export record from an ingestion result."""

        return cls(
            source_path=document.source_path,
            original_text=document.text,
            processed_text=document.text if processed_text is None else processed_text,
            document_type=document.document_type,
            support_status=support_status,
            extraction_method=document.extraction_method,
            warnings=list(document.warnings),
            metadata=dict(document.metadata),
            extracted_character_count=len(document.text),
            structured_source=structured_source,
        )


@dataclass
class ProcessedCorpusExportResult:
    """Paths and counts produced by a processed-corpus export."""

    output_directory: Path
    texts_directory: Path
    manifest_path: Path
    warnings_path: Path
    warnings_csv_path: Path
    config_path: Path
    readme_path: Optional[Path]
    files_exported: int
    warning_count: int
    manifest: Dict[str, Any]


def export_processed_corpus(
    records: Iterable[ProcessedDocumentRecord],
    processing_config: Mapping[str, Any],
    output_directory: Union[str, Path],
    *,
    app_version: Optional[str] = None,
    app_name: str = "CorpusAid",
    include_readme: bool = True,
) -> ProcessedCorpusExportResult:
    """Export processed texts and reproducibility manifests.

    The service never writes to source paths. Existing export files are treated
    as collisions and raise ``FileExistsError`` so callers can ask the user for a
    fresh output directory instead of silently replacing a previous export.
    """

    output_path = Path(output_directory)
    texts_path = output_path / "texts"
    record_list = list(records)
    config_payload = _json_compatible(processing_config)
    export_timestamp = (
        datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    )

    output_path.mkdir(parents=True, exist_ok=True)
    texts_path.mkdir(exist_ok=True)
    _ensure_metadata_paths_available(output_path, include_readme)

    support_index = _support_index()
    manifest_documents: List[Dict[str, Any]] = []
    warning_rows: List[Dict[str, Any]] = []
    allocated_paths: set[Path] = set()

    for index, record in enumerate(record_list, start=1):
        source_path = Path(record.source_path)
        support = _resolve_support(record, source_path, support_index)
        output_text_path = _allocate_text_path(
            texts_path,
            source_path,
            index,
            record.processed_text,
            allocated_paths,
        )
        if output_text_path.exists():
            raise FileExistsError(f"Refusing to overwrite existing export text: {output_text_path}")

        output_text_path.write_text(record.processed_text, encoding="utf-8")

        warnings_payload = [_warning_to_json(warning) for warning in record.warnings]
        relative_output_text_path = output_text_path.relative_to(output_path).as_posix()
        source_hash = record.source_hash or _hash_file(source_path)
        processed_text_hash = _hash_text(record.processed_text)
        extracted_count = _extracted_character_count(record)
        structured_source = (
            bool(record.structured_source)
            if record.structured_source is not None
            else _is_structured_source(source_path, support)
        )

        document_payload = {
            "source_path": str(source_path),
            "source_extension": source_path.suffix.lower(),
            "document_type": record.document_type or support.get("document_type"),
            "support_status": record.support_status or support.get("support_level"),
            "extraction_method": record.extraction_method,
            "output_text_path": relative_output_text_path,
            "source_hash": source_hash,
            "processed_text_hash": processed_text_hash,
            "original_character_count": extracted_count,
            "extracted_character_count": extracted_count,
            "processed_character_count": len(record.processed_text),
            "warnings": warnings_payload,
            "structured_source": structured_source,
            "metadata": _json_compatible(record.metadata),
        }
        manifest_documents.append(document_payload)

        for warning in warnings_payload:
            warning_rows.append(
                {
                    "source_path": str(source_path),
                    "output_text_path": relative_output_text_path,
                    "code": warning["code"],
                    "message": warning["message"],
                    "details": warning["details"],
                }
            )

    warning_count = len(warning_rows)
    manifest = {
        "app_name": app_name,
        "app_version": app_version,
        "export_timestamp": export_timestamp,
        "processing_config": config_payload,
        "files_exported": len(manifest_documents),
        "warning_count": warning_count,
        "documents": manifest_documents,
    }
    warnings_payload = {
        "warning_count": warning_count,
        "warnings": warning_rows,
    }

    config_path = output_path / "config.json"
    manifest_path = output_path / "manifest.json"
    warnings_path = output_path / "warnings.json"
    warnings_csv_path = output_path / "warnings.csv"
    readme_path = output_path / "README.txt" if include_readme else None

    _write_json(config_path, config_payload)
    _write_json(manifest_path, manifest)
    _write_json(warnings_path, warnings_payload)
    _write_warnings_csv(warnings_csv_path, warning_rows)
    if readme_path is not None:
        _write_readme(readme_path)

    return ProcessedCorpusExportResult(
        output_directory=output_path,
        texts_directory=texts_path,
        manifest_path=manifest_path,
        warnings_path=warnings_path,
        warnings_csv_path=warnings_csv_path,
        config_path=config_path,
        readme_path=readme_path,
        files_exported=len(manifest_documents),
        warning_count=warning_count,
        manifest=manifest,
    )


def _ensure_metadata_paths_available(output_path: Path, include_readme: bool) -> None:
    metadata_names = ["manifest.json", "warnings.json", "warnings.csv", "config.json"]
    if include_readme:
        metadata_names.append("README.txt")

    existing = [output_path / name for name in metadata_names if (output_path / name).exists()]
    if existing:
        joined = ", ".join(str(path) for path in existing)
        raise FileExistsError(f"Refusing to overwrite existing export metadata: {joined}")


def _support_index() -> Dict[str, Dict[str, Any]]:
    index: Dict[str, Dict[str, Any]] = {}
    for item in supported_document_formats():
        for extension in item.get("extensions", []):
            index[str(extension).lower()] = dict(item)
    return index


def _resolve_support(
    record: ProcessedDocumentRecord,
    source_path: Path,
    support_index: Mapping[str, Dict[str, Any]],
) -> Dict[str, Any]:
    support = dict(support_index.get(source_path.suffix.lower(), {}))
    if record.document_type is not None:
        support["document_type"] = record.document_type
    if record.support_status is not None:
        support["support_level"] = record.support_status
    return support


def _allocate_text_path(
    texts_path: Path,
    source_path: Path,
    index: int,
    processed_text: str,
    allocated_paths: set[Path],
) -> Path:
    stem = _safe_stem(source_path)
    digest = hashlib.sha256()
    digest.update(str(source_path).encode("utf-8", errors="surrogatepass"))
    digest.update(b"\0")
    digest.update(str(index).encode("ascii"))
    digest.update(b"\0")
    digest.update(processed_text.encode("utf-8"))
    suffix = digest.hexdigest()[:12]
    candidate = texts_path / f"{stem}-{suffix}.txt"
    collision_index = 2
    while candidate in allocated_paths or candidate.exists():
        candidate = texts_path / f"{stem}-{suffix}-{collision_index}.txt"
        collision_index += 1
    allocated_paths.add(candidate)
    return candidate


def _safe_stem(source_path: Path) -> str:
    stem = source_path.stem.strip() or "document"
    normalized = re.sub(r"[^A-Za-z0-9._-]+", "-", stem)
    normalized = re.sub(r"-{2,}", "-", normalized).strip("._-")
    return (normalized or "document")[:80]


def _hash_file(path: Path) -> Optional[HashPayload]:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as file:
            for chunk in iter(lambda: file.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError:
        return None
    return {"algorithm": "sha256", "value": digest.hexdigest()}


def _hash_text(text: str) -> HashPayload:
    return {
        "algorithm": "sha256",
        "value": hashlib.sha256(text.encode("utf-8")).hexdigest(),
    }


def _extracted_character_count(record: ProcessedDocumentRecord) -> Optional[int]:
    if record.extracted_character_count is not None:
        return record.extracted_character_count
    if record.original_text is not None:
        return len(record.original_text)
    return None


def _is_structured_source(source_path: Path, support: Mapping[str, Any]) -> bool:
    can_overwrite = support.get("can_overwrite_with_extracted_text")
    if can_overwrite is not None:
        return not bool(can_overwrite)
    return source_path.suffix.lower() != ".txt"


def _warning_to_json(warning: WarningLike) -> Dict[str, Optional[str]]:
    if isinstance(warning, ExtractionWarning):
        return warning.to_json_dict()
    if isinstance(warning, Mapping):
        return {
            "code": str(warning.get("code") or "warning"),
            "message": str(warning.get("message") or ""),
            "details": (
                None
                if warning.get("details") is None
                else str(warning.get("details"))
            ),
        }
    return {"code": "warning", "message": str(warning), "details": None}


def _json_compatible(value: Any) -> Any:
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, Mapping):
        return {str(key): _json_compatible(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_json_compatible(item) for item in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


def _write_json(path: Path, payload: Any) -> None:
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _write_warnings_csv(path: Path, rows: Sequence[Mapping[str, Any]]) -> None:
    fieldnames = ["source_path", "output_text_path", "code", "message", "details"]
    with path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow({field: row.get(field) for field in fieldnames})


def _write_readme(path: Path) -> None:
    path.write_text(
        "\n".join(
            [
                "CorpusAid processed corpus export",
                "",
                "texts/ contains one UTF-8 .txt file per exported source document.",
                "manifest.json records source provenance, extraction details, hashes, warnings, and output paths.",
                "warnings.json and warnings.csv collect extraction/export warnings.",
                "config.json records the processing parameters used for this export.",
                "",
                "Structured sources such as HTML, DOCX, and PDF are not overwritten by this export.",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
