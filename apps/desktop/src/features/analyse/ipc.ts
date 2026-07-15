/**
 * Authored Analyse gateway over Rust-generated IPC contracts.
 *
 * Generated types preserve serialised field ownership. This module supplies stable frontend names,
 * request defaults, and Tauri invocation functions used by the workbench controllers.
 */
import { invoke } from "../../app/system-ipc";
export { listen, type UnlistenFn } from "../../app/system-ipc";

import type { AnnotatedFrequencyMode as GeneratedAnnotatedFrequencyMode } from "../../generated/AnnotatedFrequencyMode.js";
import type { AnnotatedFrequencyQuery as GeneratedAnnotatedFrequencyQuery } from "../../generated/AnnotatedFrequencyQuery.js";
import type { AnnotatedFrequencyResult as GeneratedAnnotatedFrequencyResult } from "../../generated/AnnotatedFrequencyResult.js";
import type { AnnotatedTermDistributionQuery as GeneratedAnnotatedTermDistributionQuery } from "../../generated/AnnotatedTermDistributionQuery.js";
import type { AnnotatedTermSelector as GeneratedAnnotatedTermSelector } from "../../generated/AnnotatedTermSelector.js";
import type { AnnotationCacheAvailability as GeneratedAnnotationCacheAvailability } from "../../generated/AnnotationCacheAvailability.js";
import type { AnnotationCacheBuildError as GeneratedAnnotationCacheBuildError } from "../../generated/AnnotationCacheBuildError.js";
import type { AnnotationCacheBuildReport as GeneratedAnnotationCacheBuildReport } from "../../generated/AnnotationCacheBuildReport.js";
import type { AnnotationCacheBuildProgress as GeneratedAnnotationCacheBuildProgress } from "../../generated/AnnotationCacheBuildProgress.js";
import type { AnnotationCacheState as GeneratedAnnotationCacheState } from "../../generated/AnnotationCacheState.js";
import type { AnalysisDocumentSelection as GeneratedAnalysisDocumentSelection } from "../../generated/AnalysisDocumentSelection.js";
import type { CleaningConfig } from "../../generated/CleaningConfig.js";
import type {
  AnalysisInputSource as GeneratedAnalysisInputSource,
  CorpusOverview as GeneratedCorpusOverview,
  CorpusOverviewBalance as GeneratedCorpusOverviewBalance,
  CorpusOverviewBuild as GeneratedCorpusOverviewBuild,
  CorpusOverviewDispersion as GeneratedCorpusOverviewDispersion,
  CorpusOverviewDispersionPlane as GeneratedCorpusOverviewDispersionPlane,
  CorpusOverviewDocument as GeneratedCorpusOverviewDocument,
  CorpusOverviewExtractionQuality as GeneratedCorpusOverviewExtractionQuality,
  CorpusOverviewFinding as GeneratedCorpusOverviewFinding,
  CorpusOverviewFindingKind as GeneratedCorpusOverviewFindingKind,
  CorpusOverviewFindingSeverity as GeneratedCorpusOverviewFindingSeverity,
  CorpusOverviewFrequencyConcentration as GeneratedCorpusOverviewFrequencyConcentration,
  CorpusOverviewLexicalSpectrum as GeneratedCorpusOverviewLexicalSpectrum,
  CorpusOverviewMattrDistribution as GeneratedCorpusOverviewMattrDistribution,
  CorpusOverviewPopulation as GeneratedCorpusOverviewPopulation,
  CorpusOverviewRankFrequency as GeneratedCorpusOverviewRankFrequency,
  CorpusOverviewReading as GeneratedCorpusOverviewReading,
  CorpusOverviewTotals as GeneratedCorpusOverviewTotals,
  CorpusOverviewVocabularyGrowth as GeneratedCorpusOverviewVocabularyGrowth,
  CumulativeCoveragePoint as GeneratedCumulativeCoveragePoint,
  DispersionProfileBucket as GeneratedDispersionProfileBucket,
  DispersionRosterEntry as GeneratedDispersionRosterEntry,
  DispersionWitness as GeneratedDispersionWitness,
  DispersionWitnessKind as GeneratedDispersionWitnessKind,
  FrequencySpectrumPoint as GeneratedFrequencySpectrumPoint,
  LengthHistogramBin as GeneratedLengthHistogramBin,
  MattrWitness as GeneratedMattrWitness,
  RankFrequencyPoint as GeneratedRankFrequencyPoint,
  UniversalType as GeneratedUniversalType,
  VocabularyGrowthPoint as GeneratedVocabularyGrowthPoint,
} from "../../generated/CorpusOverview.js";
import type { CoverageTargetCrossing as GeneratedCoverageTargetCrossing } from "../../generated/CoverageTargetCrossing.js";
import type { CompareExpressionsQuery as GeneratedCompareExpressionsQuery } from "../../generated/CompareExpressionsQuery.js";
import type { CompareExpressionsResult as GeneratedCompareExpressionsResult } from "../../generated/CompareExpressionsResult.js";
import type { ConcordanceContextFilterDirection as GeneratedConcordanceContextFilterDirection } from "../../generated/ConcordanceContextFilterDirection.js";
import type { ConcordanceQuery as GeneratedConcordanceQuery } from "../../generated/ConcordanceQuery.js";
import type { ConcordanceExpansionQuery as GeneratedConcordanceExpansionQuery } from "../../generated/ConcordanceExpansionQuery.js";
import type { ConcordanceExpansionResult as GeneratedConcordanceExpansionResult } from "../../generated/ConcordanceExpansionResult.js";
import type { ConcordancePhase as GeneratedConcordancePhase } from "../../generated/ConcordancePhase.js";
import type { ConcordanceProgress as GeneratedConcordanceProgress } from "../../generated/ConcordanceProgress.js";
import type { ConcordanceCollocateRow as GeneratedConcordanceCollocateRow } from "../../generated/ConcordanceCollocateRow.js";
import type { ConcordanceCollocatePositionCounts as GeneratedConcordanceCollocatePositionCounts } from "../../generated/ConcordanceCollocatePositionCounts.js";
import type { ConcordanceCollocationDirection as GeneratedConcordanceCollocationDirection } from "../../generated/ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure as GeneratedConcordanceCollocationMeasure } from "../../generated/ConcordanceCollocationMeasure.js";
import type { ConcordanceCollocationQuery as GeneratedConcordanceCollocationQuery } from "../../generated/ConcordanceCollocationQuery.js";
import type { ConcordanceCollocationResult as GeneratedConcordanceCollocationResult } from "../../generated/ConcordanceCollocationResult.js";
import type { ConcordanceCollocationSortKey as GeneratedConcordanceCollocationSortKey } from "../../generated/ConcordanceCollocationSortKey.js";
import type { ConcordanceResult as GeneratedConcordanceResult } from "../../generated/ConcordanceResult.js";
import type { ConcordanceRow as GeneratedConcordanceRow } from "../../generated/ConcordanceRow.js";
import type { ConcordanceSortKey as GeneratedConcordanceSortKey } from "../../generated/ConcordanceSortKey.js";
import type { FrequencyGroupMode as GeneratedFrequencyGroupMode } from "../../generated/FrequencyGroupMode.js";
import type { FrequencyMatchMode as GeneratedFrequencyMatchMode } from "../../generated/FrequencyMatchMode.js";
import type { FrequencyPage as GeneratedFrequencyPage } from "../../generated/FrequencyPage.js";
import type { FrequencyQuery as GeneratedFrequencyQuery } from "../../generated/FrequencyQuery.js";
import type { FrequencySortKey as GeneratedFrequencySortKey } from "../../generated/FrequencySortKey.js";
import type { FrequencyTermItem as GeneratedFrequencyTermItem } from "../../generated/FrequencyTermItem.js";
import type { FrequencyTokenClass as GeneratedFrequencyTokenClass } from "../../generated/FrequencyTokenClass.js";
import type { SortDirection as GeneratedSortDirection } from "../../generated/SortDirection.js";
import type { OverviewDistributionEvidencePage as GeneratedOverviewDistributionEvidencePage } from "../../generated/OverviewDistributionEvidencePage.js";
import type { OverviewDistributionEvidenceQuery as GeneratedOverviewDistributionEvidenceQuery } from "../../generated/OverviewDistributionEvidenceQuery.js";
import type { OverviewDistributionMetric as GeneratedOverviewDistributionMetric } from "../../generated/OverviewDistributionMetric.js";
import type { OverviewDistributionQuery as GeneratedOverviewDistributionQuery } from "../../generated/OverviewDistributionQuery.js";
import type { OverviewDistributionResult as GeneratedOverviewDistributionResult } from "../../generated/OverviewDistributionResult.js";
import type { TermDistributionItem as GeneratedTermDistributionItem } from "../../generated/TermDistributionItem.js";
import type { TermDistributionPage as GeneratedTermDistributionPage } from "../../generated/TermDistributionPage.js";

export type AnalysisInputSource = GeneratedAnalysisInputSource;
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
export type CoverageTargetCrossing = GeneratedCoverageTargetCrossing;
export type OverviewDistributionMetric = GeneratedOverviewDistributionMetric;
export type OverviewDistributionQuery = GeneratedOverviewDistributionQuery;
export type OverviewDistributionResult = GeneratedOverviewDistributionResult;
export type OverviewDistributionEvidenceQuery = GeneratedOverviewDistributionEvidenceQuery;
export type OverviewDistributionEvidencePage = GeneratedOverviewDistributionEvidencePage;

export interface AnalysisDocumentSummary {
  id: number;
  sourcePath: string;
  relativePath: string;
  tokenCount: number;
}

export type CorpusOverviewFindingKind = GeneratedCorpusOverviewFindingKind;
export type CorpusOverviewFindingSeverity = GeneratedCorpusOverviewFindingSeverity;
export type CorpusOverviewTotals = GeneratedCorpusOverviewTotals;
export type CorpusOverviewPopulation = GeneratedCorpusOverviewPopulation;
export type CorpusOverviewDocument = GeneratedCorpusOverviewDocument;
export type CorpusOverviewBalance = GeneratedCorpusOverviewBalance;
export type LengthHistogramBin = GeneratedLengthHistogramBin;
export type CorpusOverviewExtractionQuality = GeneratedCorpusOverviewExtractionQuality;
export type CorpusOverviewLexicalSpectrum = GeneratedCorpusOverviewLexicalSpectrum;
export type MattrWitness = GeneratedMattrWitness;
export type CorpusOverviewMattrDistribution = GeneratedCorpusOverviewMattrDistribution;
export type CorpusOverviewFrequencyConcentration = GeneratedCorpusOverviewFrequencyConcentration;
export type CumulativeCoveragePoint = GeneratedCumulativeCoveragePoint;
export type FrequencySpectrumPoint = GeneratedFrequencySpectrumPoint;
export type RankFrequencyPoint = GeneratedRankFrequencyPoint;
export type CorpusOverviewRankFrequency = GeneratedCorpusOverviewRankFrequency;
export type VocabularyGrowthPoint = GeneratedVocabularyGrowthPoint;
export type CorpusOverviewVocabularyGrowth = GeneratedCorpusOverviewVocabularyGrowth;
export type CorpusOverviewDispersion = GeneratedCorpusOverviewDispersion;
export type DispersionProfileBucket = GeneratedDispersionProfileBucket;
export type DispersionWitnessKind = GeneratedDispersionWitnessKind;
export type DispersionWitness = GeneratedDispersionWitness;
export type CorpusOverviewDispersionPlane = GeneratedCorpusOverviewDispersionPlane;
export type DispersionRosterEntry = GeneratedDispersionRosterEntry;
export type UniversalType = GeneratedUniversalType;
export type CorpusOverviewFinding = GeneratedCorpusOverviewFinding;
export type CorpusOverviewReading = GeneratedCorpusOverviewReading;
export type CorpusOverviewBuild = GeneratedCorpusOverviewBuild;
export type CorpusOverview = GeneratedCorpusOverview;

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

export function queryOverviewDistribution(
  query: OverviewDistributionQuery,
  cleaningConfig: CleaningConfig,
): Promise<OverviewDistributionResult> {
  return invoke<OverviewDistributionResult>("query_overview_distribution_command", {
    query,
    cleaningConfig,
  });
}

export function queryOverviewDistributionEvidence(
  query: OverviewDistributionEvidenceQuery,
  cleaningConfig: CleaningConfig,
): Promise<OverviewDistributionEvidencePage> {
  return invoke<OverviewDistributionEvidencePage>(
    "query_overview_distribution_evidence_command",
    { query, cleaningConfig },
  );
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
