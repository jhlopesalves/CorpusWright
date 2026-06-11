"""Backend orchestration and extraction quality helpers."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Dict, List, Mapping, Optional, Tuple

from .models import (
    ExtractionBackendResult,
    ExtractionFallbackPolicy,
    ExtractionWarning,
)

FallbackFactory = Callable[[str], ExtractionBackendResult]


@dataclass
class OrchestratedExtraction:
    """Final text plus provenance after primary/fallback orchestration."""

    text: str
    extraction_method: str
    warnings: List[ExtractionWarning]
    metadata: Dict[str, object]


def orchestrate_extraction(
    *,
    primary: ExtractionBackendResult,
    fallback_factory: Optional[FallbackFactory],
    policy: ExtractionFallbackPolicy,
    document_type: str,
    support_status: str,
    base_metadata: Mapping[str, object],
) -> OrchestratedExtraction:
    """Choose text from primary/fallback backend and record attempts."""

    metadata = dict(base_metadata)
    attempts = [primary]
    warnings = list(primary.warnings)
    fallback_reason = _fallback_reason(primary, policy, metadata)
    fallback_attempted = False
    chosen = primary

    if fallback_reason is not None and policy.enabled:
        if fallback_factory is None:
            warnings.append(
                ExtractionWarning(
                    code="fallback_not_configured",
                    message="No fallback extraction backend is configured",
                    details=fallback_reason,
                )
            )
        else:
            fallback_attempted = True
            warnings.append(
                ExtractionWarning(
                    code="fallback_attempted",
                    message="Fallback extraction backend was attempted",
                    details=fallback_reason,
                )
            )
            fallback = fallback_factory(fallback_reason)
            attempts.append(fallback)
            warnings.extend(fallback.warnings)
            if fallback.success and fallback.text.strip():
                chosen = fallback
                warnings.append(
                    ExtractionWarning(
                        code="fallback_used",
                        message="Fallback extraction backend produced the final text",
                        details=fallback.backend_name,
                    )
                )
            elif fallback.success:
                warnings.append(
                    ExtractionWarning(
                        code="fallback_empty_extraction",
                        message="Fallback extraction produced no text",
                        details=fallback.backend_name,
                    )
                )

    quality_metadata, quality_warnings = extraction_quality(
        chosen.text,
        document_type=document_type,
        metadata={**metadata, **chosen.metadata},
        support_status=support_status,
        policy=policy,
    )
    warnings.extend(quality_warnings)

    metadata.update(chosen.metadata)
    metadata.update(quality_metadata)
    metadata.update(
        {
            "primary_backend": primary.backend_name,
            "chosen_backend": chosen.backend_name,
            "fallback_attempted": fallback_attempted,
            "fallback_reason": fallback_reason,
            "fallback_available": fallback_factory is not None,
            "support_status": support_status,
            "backend_attempts": [attempt.to_json_dict() for attempt in attempts],
        }
    )

    return OrchestratedExtraction(
        text=chosen.text,
        extraction_method=chosen.backend_name,
        warnings=warnings,
        metadata=metadata,
    )


def extraction_quality(
    text: str,
    *,
    document_type: str,
    metadata: Mapping[str, object],
    support_status: str,
    policy: Optional[ExtractionFallbackPolicy] = None,
) -> Tuple[Dict[str, object], List[ExtractionWarning]]:
    """Return common quality metadata and structured diagnostics."""

    active_policy = policy or ExtractionFallbackPolicy()
    stripped = text.strip()
    non_whitespace_count = sum(1 for char in text if not char.isspace())
    line_count = 0 if not text else len(text.splitlines()) or 1
    quality_metadata: Dict[str, object] = {
        "support_status": support_status,
        "extracted_character_count": len(text),
        "non_whitespace_character_count": non_whitespace_count,
        "line_count": line_count,
    }
    warnings: List[ExtractionWarning] = []

    if document_type == "pdf":
        if not stripped:
            warnings.append(
                ExtractionWarning(
                    code="empty_pdf_extraction",
                    message="PDF extraction produced no text",
                )
            )
            if _metadata_page_count(metadata) > 0:
                warnings.append(
                    ExtractionWarning(
                        code="pdf_suspected_scanned_or_image_only",
                        message=(
                            "PDF pages produced no embedded text; the source may be "
                            "scanned, image-only, or drawn without extractable text"
                        ),
                    )
                )
        elif non_whitespace_count < active_policy.near_empty_character_threshold:
            warnings.append(
                ExtractionWarning(
                    code="near_empty_pdf_extraction",
                    message="PDF extraction produced very little text",
                    details=f"{non_whitespace_count} non-whitespace characters",
                )
            )
        elif _low_text_yield(non_whitespace_count, metadata, active_policy):
            warnings.append(
                ExtractionWarning(
                    code="low_text_yield",
                    message="Structured source produced suspiciously little text",
                    details=(
                        f"{non_whitespace_count} non-whitespace characters from "
                        f"{metadata.get('size_bytes')} source bytes"
                    ),
                )
            )

    return quality_metadata, warnings


def _fallback_reason(
    result: ExtractionBackendResult,
    policy: ExtractionFallbackPolicy,
    metadata: Mapping[str, object],
) -> Optional[str]:
    if not policy.enabled:
        return None
    if policy.fallback_on_failure and not result.success:
        return "primary_failed"

    non_whitespace_count = sum(1 for char in result.text if not char.isspace())
    if policy.fallback_on_empty and not result.text.strip():
        return "primary_empty"
    if (
        policy.fallback_on_near_empty
        and result.text.strip()
        and non_whitespace_count < policy.near_empty_character_threshold
    ):
        return "primary_near_empty"
    if (
        policy.fallback_on_low_yield
        and result.text.strip()
        and _low_text_yield(non_whitespace_count, metadata, policy)
    ):
        return "primary_low_text_yield"
    return None


def _low_text_yield(
    non_whitespace_count: int,
    metadata: Mapping[str, object],
    policy: ExtractionFallbackPolicy,
) -> bool:
    size_bytes = metadata.get("size_bytes")
    if not isinstance(size_bytes, int):
        return False
    if size_bytes < policy.low_yield_min_source_bytes:
        return False
    return (non_whitespace_count / max(size_bytes, 1)) < policy.low_yield_ratio


def _metadata_page_count(metadata: Mapping[str, object]) -> int:
    page_count = metadata.get("page_count")
    if isinstance(page_count, int):
        return page_count
    return 0

