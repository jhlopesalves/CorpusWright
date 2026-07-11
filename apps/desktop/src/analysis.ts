import { invoke } from "@tauri-apps/api/core";

import type { AnnotatedFrequencyMode as GeneratedAnnotatedFrequencyMode } from "./generated/AnnotatedFrequencyMode.js";
import type { AnnotatedFrequencyQuery as GeneratedAnnotatedFrequencyQuery } from "./generated/AnnotatedFrequencyQuery.js";
import type { AnnotatedFrequencyResult as GeneratedAnnotatedFrequencyResult } from "./generated/AnnotatedFrequencyResult.js";
import type { AnnotatedTermDistributionQuery as GeneratedAnnotatedTermDistributionQuery } from "./generated/AnnotatedTermDistributionQuery.js";
import type { AnnotatedTermSelector as GeneratedAnnotatedTermSelector } from "./generated/AnnotatedTermSelector.js";
import type { AnnotationCacheAvailability as GeneratedAnnotationCacheAvailability } from "./generated/AnnotationCacheAvailability.js";
import type { AnnotationCacheBuildError as GeneratedAnnotationCacheBuildError } from "./generated/AnnotationCacheBuildError.js";
import type { AnnotationCacheBuildReport as GeneratedAnnotationCacheBuildReport } from "./generated/AnnotationCacheBuildReport.js";
import type { AnnotationCacheBuildProgress as GeneratedAnnotationCacheBuildProgress } from "./generated/AnnotationCacheBuildProgress.js";
import type { AnnotationCacheState as GeneratedAnnotationCacheState } from "./generated/AnnotationCacheState.js";
import type { AnalysisDocumentSelection as GeneratedAnalysisDocumentSelection } from "./generated/AnalysisDocumentSelection.js";
import type { CleaningConfig } from "./generated/CleaningConfig.js";
import type { CompareExpressionsQuery as GeneratedCompareExpressionsQuery } from "./generated/CompareExpressionsQuery.js";
import type { CompareExpressionsResult as GeneratedCompareExpressionsResult } from "./generated/CompareExpressionsResult.js";
import type { ConcordanceContextFilterDirection as GeneratedConcordanceContextFilterDirection } from "./generated/ConcordanceContextFilterDirection.js";
import type { ConcordanceQuery as GeneratedConcordanceQuery } from "./generated/ConcordanceQuery.js";
import type { ConcordanceExpansionQuery as GeneratedConcordanceExpansionQuery } from "./generated/ConcordanceExpansionQuery.js";
import type { ConcordanceExpansionResult as GeneratedConcordanceExpansionResult } from "./generated/ConcordanceExpansionResult.js";
import type { ConcordancePhase as GeneratedConcordancePhase } from "./generated/ConcordancePhase.js";
import type { ConcordanceProgress as GeneratedConcordanceProgress } from "./generated/ConcordanceProgress.js";
import type { ConcordanceCollocateRow as GeneratedConcordanceCollocateRow } from "./generated/ConcordanceCollocateRow.js";
import type { ConcordanceCollocatePositionCounts as GeneratedConcordanceCollocatePositionCounts } from "./generated/ConcordanceCollocatePositionCounts.js";
import type { ConcordanceCollocationDirection as GeneratedConcordanceCollocationDirection } from "./generated/ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure as GeneratedConcordanceCollocationMeasure } from "./generated/ConcordanceCollocationMeasure.js";
import type { ConcordanceCollocationQuery as GeneratedConcordanceCollocationQuery } from "./generated/ConcordanceCollocationQuery.js";
import type { ConcordanceCollocationResult as GeneratedConcordanceCollocationResult } from "./generated/ConcordanceCollocationResult.js";
import type { ConcordanceCollocationSortKey as GeneratedConcordanceCollocationSortKey } from "./generated/ConcordanceCollocationSortKey.js";
import type { ConcordanceResult as GeneratedConcordanceResult } from "./generated/ConcordanceResult.js";
import type { ConcordanceRow as GeneratedConcordanceRow } from "./generated/ConcordanceRow.js";
import type { ConcordanceSortKey as GeneratedConcordanceSortKey } from "./generated/ConcordanceSortKey.js";
import type { FrequencyGroupMode as GeneratedFrequencyGroupMode } from "./generated/FrequencyGroupMode.js";
import type { FrequencyMatchMode as GeneratedFrequencyMatchMode } from "./generated/FrequencyMatchMode.js";
import type { FrequencyPage as GeneratedFrequencyPage } from "./generated/FrequencyPage.js";
import type { FrequencyQuery as GeneratedFrequencyQuery } from "./generated/FrequencyQuery.js";
import type { FrequencySortKey as GeneratedFrequencySortKey } from "./generated/FrequencySortKey.js";
import type { FrequencyTermItem as GeneratedFrequencyTermItem } from "./generated/FrequencyTermItem.js";
import type { FrequencyTokenClass as GeneratedFrequencyTokenClass } from "./generated/FrequencyTokenClass.js";
import type { SortDirection as GeneratedSortDirection } from "./generated/SortDirection.js";
import type { TermDistributionItem as GeneratedTermDistributionItem } from "./generated/TermDistributionItem.js";
import type { TermDistributionPage as GeneratedTermDistributionPage } from "./generated/TermDistributionPage.js";

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
export type AnnotatedTermSelector = GeneratedAnnotatedTermSelector;
export type AnnotatedTermDistributionQuery = GeneratedAnnotatedTermDistributionQuery;
export type AnnotationCacheAvailability = GeneratedAnnotationCacheAvailability;
export type AnnotationCacheBuildError = GeneratedAnnotationCacheBuildError;
export type AnnotationCacheBuildReport = GeneratedAnnotationCacheBuildReport;
export type AnnotationCacheBuildProgress = GeneratedAnnotationCacheBuildProgress;
export type AnnotationCacheState = GeneratedAnnotationCacheState;
export type TermDistributionItem = GeneratedTermDistributionItem;
export type TermDistributionPage = GeneratedTermDistributionPage;

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
  lengthHistogram: LengthHistogramBin[];
  remainingDocuments: number;
  reading: string;
}

export interface LengthHistogramBin {
  lowerTokens: number;
  upperTokens: number | null;
  count: number;
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
  frequencySpectrumTotal: number;
  mattrWindow: number;
  mattrMedian: number | null;
  mattrMin: number | null;
  mattrMax: number | null;
  mattrDocsExcluded: number;
  mattrDocsUsed: number;
  mattrDistribution: CorpusOverviewMattrDistribution | null;
  mtldThreshold: number;
  mtldMinDocumentTokens: number;
  mtldMedian: number | null;
  mtldMin: number | null;
  mtldMax: number | null;
  mtldDocsExcluded: number;
  mtldDocsUsed: number;
  reading: string;
}

export interface MattrWitness {
  name: string;
  mattr: number;
  tokens: number;
}

export interface CorpusOverviewMattrDistribution {
  binWidth: number;
  counts: number[];
  usedDocuments: number;
  excludedDocuments: number;
  lowWitnesses: MattrWitness[];
  highWitnesses: MattrWitness[];
  reading: string;
}

export interface CorpusOverviewFrequencyConcentration {
  top10Coverage: number | null;
  top100Coverage: number | null;
  top1000Coverage: number | null;
  top10000Coverage: number | null;
  cumulativeCoverage: CumulativeCoveragePoint[];
  shannonEntropyBits: number | null;
  evennessJ: number | null;
  effectiveVocabularyEntropy: number | null;
  reading: string;
}

export interface CumulativeCoveragePoint {
  k: number;
  share: number;
}

export interface FrequencySpectrumPoint {
  frequency: number;
  typeCount: number;
}

export interface RankFrequencyPoint {
  rank: number;
  token: string | null;
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
  profileBuckets: DispersionProfileBucket[];
  plane: CorpusOverviewDispersionPlane | null;
  roster: DispersionRosterEntry[];
  dfIn1Count: number;
  dfIn2To9Count: number;
  dfIn10To99Count: number;
  dfIn100PlusCount: number;
  dfInAllCount: number;
  dfIn1SampleTypes: string[];
  dfIn2To9SampleTypes: string[];
  dfIn10To99SampleTypes: string[];
  dfIn100PlusSampleTypes: string[];
  dfInAllSampleTypes: string[];
  dfIn1Share: number | null;
  dfIn2To9Share: number | null;
  dfIn10To99Share: number | null;
  dfIn100PlusShare: number | null;
  dfInAllShare: number | null;
  dfAtLeast1pctCount: number;
  dfAtLeast1pctSampleTypes: string[];
  dfAtLeast1pctShare: number | null;
  dfAtLeast10pctCount: number;
  dfAtLeast10pctSampleTypes: string[];
  dfAtLeast10pctShare: number | null;
  dfAtLeast50pctCount: number;
  dfAtLeast50pctSampleTypes: string[];
  dfAtLeast50pctShare: number | null;
  dfAtLeast90pctCount: number;
  dfAtLeast90pctSampleTypes: string[];
  dfAtLeast90pctShare: number | null;
  universalTypes: UniversalType[];
  medianDocumentsPerType: number | null;
  reading: string;
}

export interface DispersionProfileBucket {
  label: string;
  lowerBound: number;
  upperBound: number | null;
  count: number;
  share: number | null;
  sampleTypes: string[];
}

export type DispersionWitnessKind = "core" | "loudButLocal" | "quietAndEven" | "rareAndLocal";

export interface DispersionWitness {
  token: string;
  frequency: number;
  documentFrequency: number;
  dp: number;
  kind: DispersionWitnessKind;
}

export interface CorpusOverviewDispersionPlane {
  xBins: number;
  yBins: number;
  freqLog10Max: number;
  counts: number[];
  maxCellCount: number;
  witnesses: DispersionWitness[];
  loudButLocalCount: number;
  loudButLocalMinFrequency: number;
  highDispersionThreshold: number;
  reading: string;
}

export interface DispersionRosterEntry {
  token: string;
  frequency: number;
  documentFrequency: number;
  dp: number;
}

export interface UniversalType {
  token: string;
  documentFrequency: number;
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
export type ConcordanceContextFilterDirection = GeneratedConcordanceContextFilterDirection;
export type ConcordanceQuery = GeneratedConcordanceQuery;
export type ConcordanceExpansionQuery = GeneratedConcordanceExpansionQuery;
export type ConcordanceExpansionResult = GeneratedConcordanceExpansionResult;
export type ConcordancePhase = GeneratedConcordancePhase;
export type ConcordanceProgress = GeneratedConcordanceProgress;
export type ConcordanceCollocateRow = GeneratedConcordanceCollocateRow;
export type ConcordanceCollocatePositionCounts = GeneratedConcordanceCollocatePositionCounts;
export type ConcordanceCollocationDirection = GeneratedConcordanceCollocationDirection;
export type ConcordanceCollocationMeasure = GeneratedConcordanceCollocationMeasure;
export type ConcordanceCollocationQuery = GeneratedConcordanceCollocationQuery;
export type ConcordanceCollocationResult = GeneratedConcordanceCollocationResult;
export type ConcordanceCollocationSortKey = GeneratedConcordanceCollocationSortKey;
export type ConcordanceRow = GeneratedConcordanceRow;
export type ConcordanceResult = GeneratedConcordanceResult;
export type ConcordanceSortKey = GeneratedConcordanceSortKey;
export type CompareExpressionsQuery = GeneratedCompareExpressionsQuery;
export type CompareExpressionsResult = GeneratedCompareExpressionsResult;

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

export function buildAnnotationCache(
  indices: number[] | null,
  corpusVersion: number,
  buildSignature: string,
  cleaningConfig: CleaningConfig,
): Promise<AnnotationCacheBuildReport> {
  return invoke<AnnotationCacheBuildReport>("build_annotation_cache_command", {
    request: {
      indices,
      corpusVersion,
      buildSignature,
      cleaningConfig,
    },
  });
}

export function cancelAnnotationBuild(): Promise<void> {
  return invoke("cancel_annotation_cache_build_command");
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

export function compareExpressions(
  query: CompareExpressionsQuery,
  corpusVersion: number,
  cleaningConfig: CleaningConfig,
): Promise<CompareExpressionsResult> {
  return invoke<CompareExpressionsResult>("compare_expressions_command", {
    query,
    corpusVersion,
    cleaningConfig,
  });
}

export function expandConcordanceContext(
  query: ConcordanceExpansionQuery,
  corpusVersion: number,
  cleaningConfig: CleaningConfig,
): Promise<ConcordanceExpansionResult> {
  return invoke<ConcordanceExpansionResult>("expand_concordance_context_command", {
    query,
    corpusVersion,
    cleaningConfig,
  });
}

export function cancelConcordance(): Promise<void> {
  return invoke("cancel_concordance_command");
}

export function queryConcordanceCollocations(
  query: ConcordanceCollocationQuery,
  corpusVersion: number,
  cleaningConfig: CleaningConfig,
): Promise<ConcordanceCollocationResult> {
  return invoke<ConcordanceCollocationResult>("query_concordance_collocations_command", {
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

export function queryAnnotatedTermDistribution(
  query: AnnotatedTermDistributionQuery,
  cleaningConfig: CleaningConfig,
): Promise<TermDistributionPage> {
  return invoke<TermDistributionPage>("query_annotated_term_distribution_command", {
    query,
    cleaningConfig,
  });
}
