import type { CleaningConfig, ReplacementRule } from "./generated/CleaningConfig.js";
import { createDefaultCleaningConfig } from "./config";
import type { DocumentRecord } from "./generated/DocumentRecord.js";
import type { RepeatedArtifactCandidate } from "./generated/RepeatedArtifactCandidate.js";
import type { SearchResult } from "./generated/SearchResult.js";
import type {
  VisibleFile,
} from "./types";

export const state = {
  currentCorpusVersion: 0,
  allFiles: [] as DocumentRecord[],
  visibleFiles: [] as VisibleFile[],
  selectedCorpusIndices: new Set<number>(),
  debounceTimer: null as number | null,
  currentCorpusRoot: null as string | null,
  lastSelectedCorpusIndex: null as number | null,
  previewGeneration: 0,
  activePreviewGeneration: 0,
  wordCountGeneration: 0,
  vsScrollTop: 0,
  vsContainerHeight: 0,
  currentPreviewOffset: 0,
  isFetchingPreview: false,
  activeCleaningConfig: createDefaultCleaningConfig() as CleaningConfig,
  tempRemovePatterns: [] as string[],
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
  selectedCandidateIds: new Set<string>(),
  scanWasProcessed: false,
  removalCountAtScanStart: 0,
};
