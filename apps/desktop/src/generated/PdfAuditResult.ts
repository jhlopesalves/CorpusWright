import type { PdfAuditQuality } from "./PdfAuditQuality.js";
import type { PdfAuditSuggestedProfile } from "./PdfAuditSuggestedProfile.js";

export type PdfAuditResult = { path: string, file_name: string, page_count: number | null, sampled_page_count: number, embedded_text_detected: boolean, embedded_text_chars: number, quality: PdfAuditQuality, pdfium_available: boolean, ocr_model_resources_available: boolean, ocr_full_usability_checked: boolean, degraded_fallback_used: boolean, suggested_profile: PdfAuditSuggestedProfile, warnings: Array<string>, };
