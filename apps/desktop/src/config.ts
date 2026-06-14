import type {
  CleaningConfig,
  PdfEmbeddedTextStrategy,
  PdfOcrQuality,
  PdfTextSource,
  RemovalRule,
  TableExtractionStrategy,
} from "./generated/CleaningConfig.js";

export type BooleanCleaningConfigKey = {
  [K in keyof CleaningConfig & string]: CleaningConfig[K] extends boolean ? K : never;
}[keyof CleaningConfig & string];

export const BOOLEAN_CLEANING_CONFIG_KEYS: BooleanCleaningConfigKey[] = [
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

export function normaliseCleaningConfig(raw: unknown): CleaningConfig {
  if (raw === null || raw === undefined || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("Invalid config: expected a JSON object.");
  }

  const config = createDefaultCleaningConfig();
  const obj = raw as Record<string, unknown>;

  const configAny = config as unknown as Record<string, unknown>;
  for (const configKey of BOOLEAN_CLEANING_CONFIG_KEYS) {
    const val = obj[configKey];
    if (typeof val === "boolean") {
      configAny[configKey] = val;
    }
  }

  if (typeof obj.table_extraction_strategy === "string" &&
      (ALLOWED_TABLE_STRATEGIES as ReadonlyArray<string>).includes(obj.table_extraction_strategy)) {
    config.table_extraction_strategy = obj.table_extraction_strategy as TableExtractionStrategy;
  }

  if (isPdfEmbeddedTextStrategy(obj.pdf_embedded_text_strategy)) {
    config.pdf_embedded_text_strategy = obj.pdf_embedded_text_strategy;
  }

  if (isPdfTextSource(obj.pdf_text_source)) {
    config.pdf_text_source = obj.pdf_text_source;
  }

  if (isPdfOcrQuality(obj.pdf_ocr_quality)) {
    config.pdf_ocr_quality = obj.pdf_ocr_quality;
  }

  if (Array.isArray(obj.remove_patterns) && obj.remove_patterns.every((p: unknown) => typeof p === "string")) {
    config.remove_patterns = [...obj.remove_patterns];
  }

  if (Array.isArray(obj.removal_rules) && obj.removal_rules.every(isRemovalRule)) {
    config.removal_rules = obj.removal_rules.map((rule) => ({
      ...rule,
      matcher: { ...rule.matcher },
    }));
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

  return config;
}
