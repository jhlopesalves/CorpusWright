export interface DocumentRecord {
  source_path: string;
  relative_path: string;
  document_type: string;
  size_bytes: number;
}

export interface DocumentTypeCounts {
  text: number;
  html: number;
  docx: number;
  pdf: number;
}

export interface CorpusSummary {
  root: string;
  files_discovered: number;
  files_supported: number;
  files_ignored: number;
  total_size_bytes: number;
  document_type_counts: DocumentTypeCounts;
}

export interface ScanReport {
  root: string;
  files: DocumentRecord[];
  files_discovered: number;
  files_supported: number;
  files_ignored: number;
  total_size_bytes: number;
  summary: CorpusSummary;
}

export interface CorpusLoadResult {
  report: ScanReport;
  corpusVersion: number;
}

export interface ExportReport {
  files_exported: number;
  warnings_count: number;
}

export interface PreviewWarning {
  source_path?: string;
  relative_path?: string;
  kind: string;
  message: string;
}

export interface FilePreview {
  source_path: string;
  relative_path: string;
  document_type: string;
  text: string;
  source_size_bytes: number;
  included_char_count: number;
  truncated: boolean;
  warnings: PreviewWarning[];
}

export interface CombinedPreview {
  files: FilePreview[];
  combined_text: string;
  total_files_previewed: number;
  total_characters_included: number;
  warnings: PreviewWarning[];
}

export interface VisibleFile {
  corpusIndex: number;
  record: DocumentRecord;
}

export interface SearchHit {
  corpus_index: number;
  relative_path: string;
  source_path?: string;
  context_before: string;
  match_text: string;
  context_after: string;
  file_match_index: number;
}

export interface SearchResult {
  total_matches: number;
  matching_file_indices: number[];
  returned_hits: number;
  truncated: boolean;
  hits: SearchHit[];
}

export interface RepeatedArtifactScanConfig {
  analyse_processed_text: boolean;
  include_exact_lines: boolean;
  include_normalized_lines: boolean;
  include_inline_artifacts: boolean;
  include_two_line_blocks: boolean;
  include_three_line_blocks: boolean;
  include_text_dominant: boolean;
  include_mixed_text_numbers: boolean;
  include_numeric_dominant: boolean;
  include_symbol_noise: boolean;
  min_occurrences: number;
  min_files: number;
  max_candidates: number;
  max_examples_per_candidate: number;
  min_line_chars: number;
  max_line_chars: number;
}

export interface PositionSummary {
  top_count: number;
  middle_count: number;
  bottom_count: number;
  unknown_count: number;
}

export interface RepeatedArtifactExample {
  file_name: string;
  file_path: string;
  line_number: number | null;
  page_number: number | null;
  context_before: string | null;
  matched_text: string;
  context_after: string | null;
}

export interface RepeatedArtifactCandidate {
  candidate_id: string;
  kind: string;
  display_text: string;
  normalized_key: string;
  occurrence_count: number;
  file_count: number;
  example_count: number;
  position_summary: PositionSummary;
  risk_label: string;
  content_class: string;
  raw_variant_count: number;
  raw_variant_count_is_capped: boolean;
  raw_variants: string[];
  examples: RepeatedArtifactExample[];
}

export interface RepeatedArtifactScanDiagnostics {
  files_requested: number;
  files_scanned: number;
  files_failed_extraction: number;
  files_empty_after_extraction: number;
  total_raw_lines: number;
  total_candidate_keys_before_filtering: number;
  candidates_after_min_occurrences: number;
  candidates_after_min_files: number;
  final_candidates: number;
  analysed_processed_text: boolean;
  custom_removals_active: number;
  max_examples_per_candidate: number;
}

export interface RepeatedArtifactScanReport {
  candidates: RepeatedArtifactCandidate[];
  diagnostics: RepeatedArtifactScanDiagnostics;
}
