import type { CleaningConfig, RemovalRule, ReplacementRule } from "../generated/CleaningConfig.js";
import { createDefaultCleaningConfig } from "../features/prepare/cleaning/config";
import type { DocumentRecord } from "../generated/DocumentRecord.js";
import type { ProjectSource } from "../generated/ProjectSource.js";
import type { ProjectSourceSummary } from "../generated/ProjectSourceSummary.js";
import type { RepeatedArtifactCandidate } from "../generated/RepeatedArtifactCandidate.js";
import type { SearchResult } from "../generated/SearchResult.js";
import type { CorpusOverview } from "../features/analyse/api";
import type { ActiveProject } from "../generated/ActiveProject.js";
import type {
  VisibleFile,
} from "../features/prepare/corpus/types";

export type { ActiveProject };

export type WorkbenchId = "prepare" | "analyse";

export const state = {
  activeProject: null as ActiveProject | null,
  activeWorkbench: "prepare" as WorkbenchId,
  analysisCorpus: null as CorpusOverview | null,
  sourceManifestVersion: 1,
  projectSources: [] as ProjectSource[],
  projectSourceSummary: null as ProjectSourceSummary | null,
  projectSourceQuerySummary: null as ProjectSourceSummary | null,
  projectSourcePageOffset: 0,
  projectSourcePageLimit: 200,
  projectSourceQueryGeneration: 0,
  projectSourceQueryPending: false,
  sourceSearchQuery: "",
  projectCorpusFileCount: 0,
  currentCorpusVersion: 0,
  allFiles: [] as DocumentRecord[],
  cleanedTokenCountText: null as string | null,
  visibleFiles: [] as VisibleFile[],
  selectedCorpusIndices: new Set<number>(),
  selectedProjectSourceIds: new Set<string>(),
  selectedProjectSourcesById: new Map<string, ProjectSource>(),
  allMatchingProjectSourcesSelected: false,
  deselectedProjectSourceIds: new Set<string>(),
  focusedProjectSourceId: null as string | null,
  focusedProjectSourceOffset: null as number | null,
  debounceTimer: null as number | null,
  currentCorpusRoot: null as string | null,
  lastSelectedCorpusIndex: null as number | null,
  lastSelectedProjectSourceId: null as string | null,
  previewGeneration: 0,
  activePreviewGeneration: 0,
  projectSourcePreviewScrollTop: 0,
  wordCountGeneration: 0,
  vsScrollTop: 0,
  vsContainerHeight: 0,
  currentPreviewOffset: 0,
  isFetchingPreview: false,
  activeCleaningConfig: createDefaultCleaningConfig() as CleaningConfig,
  tempRemovePatterns: [] as string[],
  tempRemovalRules: [] as RemovalRule[],
  tempReplacePatterns: [] as ReplacementRule[],
  currentMatchIndex: -1,
  currentSearchQuery: "",
  lastSearchedQuery: "",
  isSearching: false,
  searchGeneration: 0,
  pendingSearchAfterCurrent: false,
  pendingSearchNavigation: 0 as -1 | 0 | 1,
  searchDebounceTimer: undefined as number | undefined,
  lastSearchResult: null as SearchResult | null,
  lastScanCandidates: [] as RepeatedArtifactCandidate[],
  scanWasProcessed: false,
  removalCountAtScanStart: 0,
};
