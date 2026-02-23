import { invoke } from "@tauri-apps/api/core";

import type { AnnotatedFrequencyMode as GeneratedAnnotatedFrequencyMode } from "./generated/AnnotatedFrequencyMode.js";
import type { AnnotatedFrequencyQuery as GeneratedAnnotatedFrequencyQuery } from "./generated/AnnotatedFrequencyQuery.js";
import type { AnnotatedFrequencyResult as GeneratedAnnotatedFrequencyResult } from "./generated/AnnotatedFrequencyResult.js";
import type { AnnotationCacheAvailability as GeneratedAnnotationCacheAvailability } from "./generated/AnnotationCacheAvailability.js";
import type { AnnotationCacheState as GeneratedAnnotationCacheState } from "./generated/AnnotationCacheState.js";
import type { AnalysisDocumentSelection as GeneratedAnalysisDocumentSelection } from "./generated/AnalysisDocumentSelection.js";
import type { CleaningConfig } from "./generated/CleaningConfig.js";
import type { ConcordanceQuery as GeneratedConcordanceQuery } from "./generated/ConcordanceQuery.js";
import type { ConcordanceResult as GeneratedConcordanceResult } from "./generated/ConcordanceResult.js";
import type { ConcordanceRow as GeneratedConcordanceRow } from "./generated/ConcordanceRow.js";
import type { FrequencyGroupMode as GeneratedFrequencyGroupMode } from "./generated/FrequencyGroupMode.js";
import type { FrequencyMatchMode as GeneratedFrequencyMatchMode } from "./generated/FrequencyMatchMode.js";
import type { FrequencyPage as GeneratedFrequencyPage } from "./generated/FrequencyPage.js";
import type { FrequencyQuery as GeneratedFrequencyQuery } from "./generated/FrequencyQuery.js";
import type { FrequencySortKey as GeneratedFrequencySortKey } from "./generated/FrequencySortKey.js";
import type { FrequencyTermItem as GeneratedFrequencyTermItem } from "./generated/FrequencyTermItem.js";
import type { FrequencyTokenClass as GeneratedFrequencyTokenClass } from "./generated/FrequencyTokenClass.js";
import type { SortDirection as GeneratedSortDirection } from "./generated/SortDirection.js";

export type AnalysisInputSource =
  | "processed"
  | "processedWithSourceTextFallback"
  | "sourceTextFallback";
export type AnalysisBuildPhase =
  | "loadingSources"
  | "readingFiles"
  | "tokenising"
  | "computingDispersion"
  | "finalisingSummaries";
export type AnalysisBuildStateKind = "notBuilt" | "building" | "ready" | "stale" | "failed" | "cancelled";
export type FrequencySortKey = GeneratedFrequencySortKey;
export type FrequencyMatchMode = GeneratedFrequencyMatchMode;
export type FrequencyTokenClass = GeneratedFrequencyTokenClass;
export type FrequencyGroupMode = GeneratedFrequencyGroupMode;
export type SortDirection = GeneratedSortDirection;
export type AnnotatedFrequencyMode = GeneratedAnnotatedFrequencyMode;
export type AnnotatedFrequencyQuery = GeneratedAnnotatedFrequencyQuery;
export type AnnotatedFrequencyResult = GeneratedAnnotatedFrequencyResult;
export type AnnotationCacheAvailability = GeneratedAnnotationCacheAvailability;
export type AnnotationCacheState = GeneratedAnnotationCacheState;

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
  processedUsed: number;
  usingSourceTextFallback: number;
  emptyOrFailed: number;
  sourcesWithWarnings: number | null;
  inputSource: string;
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
  frequencySpectrum: FrequencySpectrumPoint[];
  frequencySpectrumTruncated: boolean;
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
  shannonEntropyBits: number | null;
  evennessJ: number | null;
  perplexity: number | null;
  effectiveVocabularyEntropy: number | null;
  reading: string;
}

export interface FrequencySpectrumPoint {
  frequency: number;
  typeCount: number;
}

export interface RankFrequencyPoint {
  rank: number;
  frequency: number;
  ratePerMillion: number;
}

export interface CorpusOverviewRankFrequency {
  zipfExponent: number | null;
  zipfCoefficient: number | null;
  rSquared: number | null;
  fittedRankStart: number | null;
  fittedRankEnd: number | null;
  points: RankFrequencyPoint[];
  reading: string;
}

export interface VocabularyGrowthPoint {
  tokens: number;
  types: number;
}

export interface CorpusOverviewVocabularyGrowth {
  heapsK: number | null;
  heapsBeta: number | null;
  rSquared: number | null;
  points: VocabularyGrowthPoint[];
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
  annotationAvailable: boolean;
  inputTextSource: AnalysisInputSource;
  builtAt: string;
  buildSignature: string;
  processingConfigSignature: string;
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
  rankFrequency: CorpusOverviewRankFrequency;
  vocabularyGrowth: CorpusOverviewVocabularyGrowth;
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

export type AnalysisDocumentSelection = GeneratedAnalysisDocumentSelection;
export type FrequencyQuery = GeneratedFrequencyQuery;
export type FrequencyTermItem = GeneratedFrequencyTermItem;
export type FrequencyPage = GeneratedFrequencyPage;
export type ConcordanceQuery = GeneratedConcordanceQuery;
export type ConcordanceRow = GeneratedConcordanceRow;
export type ConcordanceResult = GeneratedConcordanceResult;

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
  ratePerMillion: number;
}

export interface TermDistributionPage {
  token: string;
  totalDocuments: number;
  offset: number;
  limit: number;
  items: TermDistributionItem[];
  maxRatePer10k: number;
  maxRatePerMillion: number;
}

export function defaultAnalysisDocumentSelection(): AnalysisDocumentSelection {
  return { allDocuments: true, ids: [], excludedIds: [] };
}

export function buildAnalysisCorpus(
  indices: number[] | null,
  corpusVersion: number,
  buildSignature: string,
  cleaningConfig: CleaningConfig,
): Promise<CorpusOverview> {
  return invoke<CorpusOverview>("build_analysis_corpus_command", {
    request: {
      indices,
      corpusVersion,
      buildSignature,
      cleaningConfig,
    },
  });
}

export function cancelAnalysisBuild(): Promise<void> {
  return invoke("cancel_analysis_build_command");
}

export function getAnalysisBuildStatus(cleaningConfig: CleaningConfig): Promise<AnalysisBuildStatus> {
  return invoke<AnalysisBuildStatus>("get_analysis_build_status_command", { cleaningConfig });
}

export function computeCorpusOverview(cleaningConfig: CleaningConfig): Promise<CorpusOverview> {
  return invoke<CorpusOverview>("compute_corpus_overview_command", { cleaningConfig });
}

export function queryFrequencyList(
  query: FrequencyQuery,
  cleaningConfig: CleaningConfig,
): Promise<FrequencyPage> {
  return invoke<FrequencyPage>("query_frequency_list_command", { query, cleaningConfig });
}

export function queryAnnotatedFrequency(
  query: AnnotatedFrequencyQuery,
  corpusVersion: number,
  cleaningConfig: CleaningConfig,
): Promise<AnnotatedFrequencyResult> {
  return invoke<AnnotatedFrequencyResult>("query_annotated_frequency_command", {
    query,
    corpusVersion,
    cleaningConfig,
  });
}

export function searchConcordance(
  query: ConcordanceQuery,
  corpusVersion: number,
  cleaningConfig: CleaningConfig,
): Promise<ConcordanceResult> {
  return invoke<ConcordanceResult>("search_concordance_command", {
    query,
    corpusVersion,
    cleaningConfig,
  });
}

export function queryAnalysisDocuments(
  selection: AnalysisDocumentSelection,
  query: string,
  offset: number,
  limit: number,
  cleaningConfig: CleaningConfig,
): Promise<AnalysisDocumentPage> {
  return invoke<AnalysisDocumentPage>("query_analysis_documents_command", {
    selection,
    query,
    offset,
    limit,
    cleaningConfig,
  });
}

export function queryTermDistribution(
  token: string,
  selection: AnalysisDocumentSelection,
  groupBy: FrequencyGroupMode,
  caseFold: boolean,
  language: string | null,
  offset: number,
  limit: number,
  cleaningConfig: CleaningConfig,
): Promise<TermDistributionPage> {
  return invoke<TermDistributionPage>("query_term_distribution_command", {
    token,
    selection,
    groupBy,
    caseFold,
    language,
    offset,
    limit,
    cleaningConfig,
  });
}
