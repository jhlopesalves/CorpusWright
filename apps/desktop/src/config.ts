import type {
  CleaningConfig,
  PdfEmbeddedTextStrategy,
  PdfOcrQuality,
  PdfTextSource,
  RemovalRule,
  TableExtractionStrategy,
} from "./generated/CleaningConfig.js";
import {
  emptyRemovalRuleMigrationSummary,
  migrateRemovalRules,
  type RemovalRuleMigrationSummary,
} from "./removal-rule-migration.ts";

export type BooleanCleaningConfigKey = {
  [K in keyof CleaningConfig & string]: CleaningConfig[K] extends boolean ? K : never;
}[keyof CleaningConfig & string];

export const BOOLEAN_CLEANING_CONFIG_KEYS: BooleanCleaningConfigKey[] = [
  "normalize_whitespace_line_endings",
  "expand_ligatures",
  "remove_invisible_controls",
  "repair_wrapped_pdf_line_breaks",
  "remove_isolated_page_markers",
  "remove_url_only_lines",
  "remove_email_only_lines",
  "remove_doi_only_lines",
  "remove_figure_table_captions",
  "join_line_breaks",
  "normalize_irregular_line_breaks",
  "remove_standalone_page_numbers",
  "remove_standalone_roman_page_numbers",
  "remove_page_indicators",
  "remove_page_delimiters",
  "lowercase",
  "normalize_line_endings",
  "trim_lines",
  "collapse_blank_lines",
  "normalize_unicode",
  "replace_diacritics",
  "extract_html",
  "remove_obvious_extraction_noise",
  "remove_headers",
  "remove_footers",
  "remove_footnotes",
  "remove_endnotes",
  "remove_comments",
  "remove_table_of_contents",
  "remove_repeated_pdf_headers_footers",
  "remove_pdf_page_labels",
  "remove_pdf_symbol_heavy_artifacts",
  "remove_pdf_code_like_blocks",
  "remove_pdf_formula_like_lines",
];

export const ALLOWED_TABLE_STRATEGIES = ["TabSeparated", "FlattenParagraphs", "Ignore"] as const;

export const ALLOWED_PDF_EMBEDDED_TEXT_STRATEGIES = [
  "PdfiumFlat",
  "PdfiumVisualSingleColumn",
  "PdfiumVisualColumnsExperimental",
] as const;

export const ALLOWED_PDF_TEXT_SOURCES = ["EmbeddedText", "Ocr", "ForceOcr"] as const;

export const ALLOWED_PDF_OCR_QUALITIES = ["Fast", "Balanced", "HighQuality"] as const;
export const ALLOWED_REMOVAL_RULE_SOURCES = [
  "manual",
  "promoted_repeated_artifact",
  "generated_pdf_cleanup",
] as const;
export const ALLOWED_REMOVAL_SCOPES = ["anywhere", "whole_line", "page_top", "page_bottom", "page_top_or_bottom"] as const;

export function createDefaultCleaningConfig(): CleaningConfig {
  return {
    normalize_whitespace_line_endings: false,
    expand_ligatures: false,
    remove_invisible_controls: false,
    repair_wrapped_pdf_line_breaks: false,
    remove_isolated_page_markers: false,
    remove_url_only_lines: false,
    remove_email_only_lines: false,
    remove_doi_only_lines: false,
    remove_figure_table_captions: false,
    join_line_breaks: false,
    normalize_irregular_line_breaks: false,
    remove_standalone_page_numbers: false,
    remove_standalone_roman_page_numbers: false,
    remove_page_indicators: false,
    remove_page_delimiters: false,
    lowercase: false,
    normalize_line_endings: false,
    trim_lines: false,
    collapse_blank_lines: false,
    normalize_unicode: false,
    replace_diacritics: false,
    extract_html: false,
    remove_obvious_extraction_noise: false,
    table_extraction_strategy: "TabSeparated" as TableExtractionStrategy,
    remove_headers: false,
    remove_footers: false,
    remove_footnotes: false,
    remove_endnotes: false,
    remove_comments: false,
    remove_table_of_contents: false,
    remove_patterns: [],
    removal_rules: [],
    replace_patterns: [],
    pdf_text_source: "EmbeddedText",
    pdf_ocr_quality: "Balanced",
    pdf_embedded_text_strategy: "PdfiumFlat",
    remove_repeated_pdf_headers_footers: false,
    remove_pdf_page_labels: false,
    remove_pdf_symbol_heavy_artifacts: false,
    remove_pdf_code_like_blocks: false,
    remove_pdf_formula_like_lines: false
  };
}

export function isPdfTextSource(value: unknown): value is PdfTextSource {
  return (
    typeof value === "string" &&
    (ALLOWED_PDF_TEXT_SOURCES as readonly string[]).includes(value)
  );
}

export function normalisePdfTextSource(value: PdfTextSource): PdfTextSource {
  return value === "Ocr" ? "ForceOcr" : value;
}

export function isPdfOcrQuality(value: unknown): value is PdfOcrQuality {
  return (
    typeof value === "string" &&
    (ALLOWED_PDF_OCR_QUALITIES as readonly string[]).includes(value)
  );
}

export function isPdfEmbeddedTextStrategy(value: unknown): value is PdfEmbeddedTextStrategy {
  return (
    typeof value === "string" &&
    (ALLOWED_PDF_EMBEDDED_TEXT_STRATEGIES as readonly string[]).includes(value)
  );
}

function isRemovalRuleSource(value: unknown): value is RemovalRule["source"] {
  return (
    typeof value === "string" &&
    (ALLOWED_REMOVAL_RULE_SOURCES as readonly string[]).includes(value)
  );
}

function isRemovalScope(value: unknown): value is RemovalRule["scope"] {
  return (
    typeof value === "string" &&
    (ALLOWED_REMOVAL_SCOPES as readonly string[]).includes(value)
  );
}

function isRemovalMatcher(value: unknown): value is RemovalRule["matcher"] {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }

  const matcherObj = value as Record<string, unknown>;
  if (matcherObj.kind === "literal") {
    return typeof matcherObj.text === "string";
  }
  if (matcherObj.kind === "normalized_line") {
    return typeof matcherObj.normalized_key === "string";
  }
  return false;
}

function isRemovalRule(value: unknown): value is RemovalRule {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }

  const obj = value as Record<string, unknown>;
  return (
    typeof obj.id === "string" &&
    typeof obj.label === "string" &&
    isRemovalRuleSource(obj.source) &&
    isRemovalScope(obj.scope) &&
    typeof obj.enabled === "boolean" &&
    isRemovalMatcher(obj.matcher)
  );
}

export interface NormalisedCleaningConfigResult {
  config: CleaningConfig;
  migrationSummary: RemovalRuleMigrationSummary;
}

export function normaliseCleaningConfigWithMigration(raw: unknown): NormalisedCleaningConfigResult {
  if (raw === null || raw === undefined || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("Invalid config: expected a JSON object.");
  }

  const config = createDefaultCleaningConfig();
  let migrationSummary = emptyRemovalRuleMigrationSummary();
  const obj = raw as Record<string, unknown>;

  const configAny = config as unknown as Record<string, unknown>;
  for (const configKey of BOOLEAN_CLEANING_CONFIG_KEYS) {
    const val = obj[configKey];
    if (typeof val === "boolean") {
      configAny[configKey] = val;
    }
  }

  if (
    obj.remove_standalone_page_numbers === true ||
    obj.remove_standalone_roman_page_numbers === true ||
    obj.remove_page_indicators === true ||
    obj.remove_page_delimiters === true ||
    obj.remove_pdf_page_labels === true
  ) {
    config.remove_isolated_page_markers = true;
  }

  if (
    obj.normalize_line_endings === true ||
    obj.trim_lines === true ||
    obj.collapse_blank_lines === true
  ) {
    config.normalize_whitespace_line_endings = true;
  }

  if (obj.join_line_breaks === true || obj.normalize_irregular_line_breaks === true) {
    config.repair_wrapped_pdf_line_breaks = true;
  }

  if (obj.remove_pdf_symbol_heavy_artifacts === true) {
    config.remove_obvious_extraction_noise = true;
  }

  if (typeof obj.table_extraction_strategy === "string" &&
      (ALLOWED_TABLE_STRATEGIES as ReadonlyArray<string>).includes(obj.table_extraction_strategy)) {
    config.table_extraction_strategy = obj.table_extraction_strategy as TableExtractionStrategy;
  }

  if (isPdfEmbeddedTextStrategy(obj.pdf_embedded_text_strategy)) {
    config.pdf_embedded_text_strategy = obj.pdf_embedded_text_strategy;
  }

  if (isPdfTextSource(obj.pdf_text_source)) {
    config.pdf_text_source = normalisePdfTextSource(obj.pdf_text_source);
  }

  if (isPdfOcrQuality(obj.pdf_ocr_quality)) {
    config.pdf_ocr_quality = obj.pdf_ocr_quality;
  }

  if (Array.isArray(obj.remove_patterns) && obj.remove_patterns.every((p: unknown) => typeof p === "string")) {
    config.remove_patterns = [...obj.remove_patterns];
  }

  if (Array.isArray(obj.removal_rules) && obj.removal_rules.every(isRemovalRule)) {
    const migrated = migrateRemovalRules(obj.removal_rules.map((rule) => ({
      ...rule,
      matcher: { ...rule.matcher },
    })));
    config.removal_rules = migrated.rules;
    migrationSummary = migrated.summary;
  }

  if (Array.isArray(obj.replace_patterns) &&
      obj.replace_patterns.every(
        (r: unknown) =>
          typeof r === "object" &&
          r !== null &&
          typeof (r as Record<string, unknown>).pattern === "string" &&
          typeof (r as Record<string, unknown>).replacement === "string"
      )) {
    config.replace_patterns = [...obj.replace_patterns];
  }

  return { config, migrationSummary };
}

export function normaliseCleaningConfig(raw: unknown): CleaningConfig {
  return normaliseCleaningConfigWithMigration(raw).config;
}
