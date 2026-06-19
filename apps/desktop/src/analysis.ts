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

export type CorpusOverviewFindingKind = "balance" | "extractionQuality" | "lexicalSpectrum" | "dispersion";
export type CorpusOverviewFindingSeverity = "noIssue" | "worthALook" | "needsFixing";

export interface CorpusOverviewTotals {
  tokens: number;
  types: number;
  documents: number;
}

export interface CorpusOverviewPopulation {
  sourcesInProject: number | null;
  includedInAnalysis: number;
  processedTextAvailable: number | null;
  usingExtractedFallback: number;
  emptyOrFailed: number;
  sourcesWithWarnings: number | null;
  inputSource: AnalysisInputSource;
  reading: string;
}

export interface CorpusOverviewDocument {
  name: string;
  tokens: number;
  share: number | null;
  relativeLength: number | null;
}

export interface CorpusOverviewBalance {
  largestShare: number | null;
  top3Share: number | null;
  gini: number | null;
  coefficientOfVariation: number | null;
  effectiveDocuments: number | null;
  medianTokens: number | null;
  minimumTokens: number | null;
  maximumTokens: number | null;
  remainingDocuments: number;
  reading: string;
}

export interface CorpusOverviewExtractionQuality {
  wordLikeCount: number;
  wordLikeShare: number | null;
  numericOnlyCount: number;
  numericOnlyShare: number | null;
  singleCharacterCount: number | null;
  singleCharacterShare: number | null;
  reading: string;
}

export interface CorpusOverviewLexicalSpectrum {
  types: number;
  hapaxCount: number;
  disLegomenaCount: number;
  band3To9Count: number;
  band10plusCount: number;
  hapaxShare: number | null;
  disLegomenaShare: number | null;
  band3To9Share: number | null;
  band10plusShare: number | null;
  rawTtr: number | null;
  mattrWindow: number;
  mattrMedian: number | null;
  mattrMin: number | null;
  mattrMax: number | null;
  mattrDocsExcluded: number;
  mattrDocsUsed: number;
  mtldThreshold: number;
  mtldMinDocumentTokens: number;
  mtldMedian: number | null;
  mtldMin: number | null;
  mtldMax: number | null;
  mtldDocsExcluded: number;
  mtldDocsUsed: number;
  reading: string;
}

export interface CorpusOverviewFrequencyConcentration {
  top100Coverage: number | null;
  top1000Coverage: number | null;
  top10000Coverage: number | null;
  evennessJ: number | null;
  effectiveVocabularyEntropy: number | null;
  reading: string;
}

export interface CorpusOverviewDispersion {
  frequentTypes: number;
  highDpFrequentTypes: number;
  medianDp: number | null;
  dfIn1Count: number;
  dfIn2ToNMinus1Count: number;
  dfInAllCount: number;
  dfIn1Share: number | null;
  dfIn2ToNMinus1Share: number | null;
  dfInAllShare: number | null;
  medianDocumentsPerType: number | null;
  reading: string;
}

export interface CorpusOverviewFinding {
  key: CorpusOverviewFindingKind;
  severity: CorpusOverviewFindingSeverity;
  severityLabel: string;
  text: string;
}

export interface CorpusOverviewReading {
  verdict: string;
  findings: CorpusOverviewFinding[];
}

export interface CorpusOverviewBuild {
  tokenisation: string | null;
  caseHandling: string | null;
  lemmatisation: string | null;
  numberPunctuationHandling: string | null;
  inputSource: string;
  configVersion: string | null;
  snapshotDate: string | null;
  metadataAttached: boolean;
}

export interface CorpusOverview {
  documentCount: number;
  sourceCount: number;
  totalTokens: number;
  totalTypes: number;
  inputTextSource: AnalysisInputSource;
  builtAt: string;
  buildSignature: string;
  skippedDocuments: number;
  skippedPdfDocuments: number;
  totals: CorpusOverviewTotals;
  population: CorpusOverviewPopulation;
  documents: CorpusOverviewDocument[];
  documentsTruncated: boolean;
  balance: CorpusOverviewBalance;
  extractionQuality: CorpusOverviewExtractionQuality;
  lexicalSpectrum: CorpusOverviewLexicalSpectrum;
  frequencyConcentration: CorpusOverviewFrequencyConcentration;
  dispersion: CorpusOverviewDispersion;
  reading: CorpusOverviewReading;
  build: CorpusOverviewBuild;
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
  overview: CorpusOverview | null;
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
): Promise<CorpusOverview> {
  return invoke<CorpusOverview>("build_analysis_corpus_command", {
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

export function computeCorpusOverview(): Promise<CorpusOverview> {
  return invoke<CorpusOverview>("compute_corpus_overview_command");
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
