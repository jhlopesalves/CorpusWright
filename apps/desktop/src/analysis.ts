import { invoke } from "@tauri-apps/api/core";

import type { CleaningConfig } from "./generated/CleaningConfig.js";

export type AnalysisInputSource = "processed" | "extracted";
export type AnalysisBuildPhase =
  | "loadingSources"
  | "readingFiles"
  | "tokenising"
  | "computingDispersion"
  | "finalisingSummaries";
export type AnalysisBuildStateKind = "notBuilt" | "building" | "ready" | "failed" | "cancelled";
export type FrequencySortKey = "token" | "frequency" | "range" | "rate" | "dispersion";
export type FrequencyMatchMode = "contains" | "exact" | "startsWith" | "regex";
export type FrequencyTokenClass = "alphabetic" | "includeSingles" | "includeNumerals" | "all";
export type SortDirection = "asc" | "desc";

export interface AnalysisDocumentSummary {
  id: number;
  sourcePath: string;
  relativePath: string;
  tokenCount: number;
}

export interface AnalysisBalanceSummary {
  largestDocument: AnalysisDocumentSummary | null;
  smallestNonEmptyDocument: AnalysisDocumentSummary | null;
  minTokens: number | null;
  maxTokens: number | null;
  medianTokens: number | null;
  q1Tokens: number | null;
  q3Tokens: number | null;
  iqrTokens: number | null;
  largestDocumentShare: number | null;
  topDocumentCount: number;
  topDocumentShare: number | null;
  lengthRatio: number | null;
  effectiveDocumentCount: number | null;
}

export interface AnalysisLexicalSummary {
  uniqueTypes: number;
  hapaxLegomena: number;
  disLegomena: number;
  repeatedTypes: number;
  numericOnlyTypes: number;
  numericOnlyShare: number | null;
  singleCharacterTypes: number;
  singleCharacterShare: number | null;
  qualityOtherTypes: number;
  typeTokenRatio: number | null;
}

export interface AnalysisDispersionSummary {
  documentCount: number;
  allDocumentTypes: number;
  oneDocumentTypes: number;
  frequentTypes: number;
  medianFrequentDispersion: number | null;
  highDispersionFrequentTypes: number;
}

export interface AnalysisOverview {
  documentCount: number;
  sourceCount: number;
  totalTokens: number;
  totalTypes: number;
  inputTextSource: AnalysisInputSource;
  builtAt: string;
  buildSignature: string;
  skippedDocuments: number;
  skippedPdfDocuments: number;
  balance: AnalysisBalanceSummary;
  lexical: AnalysisLexicalSummary;
  dispersion: AnalysisDispersionSummary;
  documentLengths: AnalysisDocumentSummary[];
  documentLengthsTruncated: boolean;
}

export interface AnalysisBuildProgress {
  filesDone: number;
  filesTotal: number;
  tokensSeen: number;
  typesSeen: number;
  currentPhase: AnalysisBuildPhase;
  elapsedMs: number;
}

export interface AnalysisBuildStatus {
  state: AnalysisBuildStateKind;
  progress: AnalysisBuildProgress | null;
  error: string | null;
  overview: AnalysisOverview | null;
}

export interface AnalysisDocumentSelection {
  allDocuments: boolean;
  ids: number[];
  excludedIds: number[];
}

export interface FrequencyQuery {
  selection: AnalysisDocumentSelection;
  query: string;
  matchMode: FrequencyMatchMode;
  minimumFrequency: number;
  tokenClass: FrequencyTokenClass;
  hideStopwords: boolean;
  sortKey: FrequencySortKey;
  sortDirection: SortDirection;
  offset: number;
  limit: number;
}

export interface FrequencyTermItem {
  token: string;
  frequency: number;
  documentFrequency: number;
  ratePer10k: number;
  dispersion: number;
}

export interface FrequencyPage {
  totalTerms: number;
  scopedTotalTypes: number;
  offset: number;
  limit: number;
  items: FrequencyTermItem[];
  selectedDocumentCount: number;
  totalDocumentCount: number;
  scopedTotalTokens: number;
  queryError: string | null;
}

export interface AnalysisDocumentPageItem extends AnalysisDocumentSummary {
  selected: boolean;
}

export interface AnalysisDocumentPage {
  totalCount: number;
  offset: number;
  limit: number;
  items: AnalysisDocumentPageItem[];
  selectedCount: number;
  totalDocumentCount: number;
}

export interface TermDistributionItem {
  documentId: number;
  sourcePath: string;
  relativePath: string;
  tokenCount: number;
  count: number;
  ratePer10k: number;
}

export interface TermDistributionPage {
  token: string;
  totalDocuments: number;
  offset: number;
  limit: number;
  items: TermDistributionItem[];
  maxRatePer10k: number;
}

export function defaultAnalysisDocumentSelection(): AnalysisDocumentSelection {
  return { allDocuments: true, ids: [], excludedIds: [] };
}

export function buildAnalysisCorpus(
  indices: number[] | null,
  corpusVersion: number,
  inputTextSource: AnalysisInputSource,
  buildSignature: string,
  cleaningConfig: CleaningConfig,
): Promise<AnalysisOverview> {
  return invoke<AnalysisOverview>("build_analysis_corpus_command", {
    request: {
      indices,
      corpusVersion,
      inputTextSource,
      buildSignature,
      cleaningConfig,
    },
  });
}

export function cancelAnalysisBuild(): Promise<void> {
  return invoke("cancel_analysis_build_command");
}

export function getAnalysisBuildStatus(): Promise<AnalysisBuildStatus> {
  return invoke<AnalysisBuildStatus>("get_analysis_build_status_command");
}

export function getAnalysisOverview(): Promise<AnalysisOverview> {
  return invoke<AnalysisOverview>("get_analysis_overview_command");
}

export function queryFrequencyList(query: FrequencyQuery): Promise<FrequencyPage> {
  return invoke<FrequencyPage>("query_frequency_list_command", { query });
}

export function queryAnalysisDocuments(
  selection: AnalysisDocumentSelection,
  query: string,
  offset: number,
  limit: number,
): Promise<AnalysisDocumentPage> {
  return invoke<AnalysisDocumentPage>("query_analysis_documents_command", {
    selection,
    query,
    offset,
    limit,
  });
}

export function queryTermDistribution(
  token: string,
  selection: AnalysisDocumentSelection,
  offset: number,
  limit: number,
): Promise<TermDistributionPage> {
  return invoke<TermDistributionPage>("query_term_distribution_command", {
    token,
    selection,
    offset,
    limit,
  });
}
