function getElement<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

export const dom = {
  welcomeScreen: getElement<HTMLElement>("welcome-screen"),
  projectWorkspace: getElement<HTMLElement>("project-workspace"),
  prepareWorkbench: getElement<HTMLElement>("prepare-workbench"),
  analyseWorkbench: getElement<HTMLElement>("analyse-workbench"),
  workbenchSwitcher: getElement<HTMLDivElement>("workbench-switcher"),
  workbenchPrepare: getElement<HTMLButtonElement>("workbench-prepare"),
  workbenchAnalyse: getElement<HTMLButtonElement>("workbench-analyse"),
  workbenchExport: getElement<HTMLButtonElement>("workbench-export"),
  stagePrepareStatus: getElement<HTMLSpanElement>("stage-prepare-status"),
  stageAnalyseStatus: getElement<HTMLSpanElement>("stage-analyse-status"),
  stageExportStatus: getElement<HTMLSpanElement>("stage-export-status"),
  analyseStatusChip: getElement<HTMLSpanElement>("analyse-status-chip"),
  analyseStatusDetail: getElement<HTMLParagraphElement>("analyse-status-detail"),
  analyseBuildButton: getElement<HTMLButtonElement>("analyse-build-button"),
  analyseBuildNote: getElement<HTMLSpanElement>("analyse-build-note"),
  analyseInputBanner: getElement<HTMLDivElement>("analyse-input-banner"),
  analyseProjectSummary: getElement<HTMLDListElement>("analyse-project-summary"),
  analyseToolsList: getElement<HTMLUListElement>("analyse-tools-list"),
  analyseOverviewPane: getElement<HTMLElement>("analyse-overview-pane"),
  analyseOverview: getElement<HTMLDivElement>("analyse-overview"),
  frequencySearchInput: getElement<HTMLInputElement>("frequency-search-input"),
  frequencyMatchModeSelect: getElement<HTMLSelectElement>("frequency-match-mode"),
  frequencySearchError: getElement<HTMLDivElement>("frequency-search-error"),
  frequencyMinInput: getElement<HTMLInputElement>("frequency-min-input"),
  frequencyTokenClassSelect: getElement<HTMLSelectElement>("frequency-token-class"),
  frequencyHideStopwordsInput: getElement<HTMLInputElement>("frequency-hide-stopwords"),
  frequencyFilesButton: getElement<HTMLButtonElement>("frequency-files-button"),
  frequencySection: getElement<HTMLElement>("frequency-list-pane"),
  frequencySummary: getElement<HTMLParagraphElement>("frequency-summary"),
  frequencyResultCount: getElement<HTMLSpanElement>("frequency-result-count"),
  frequencyLimitNote: getElement<HTMLSpanElement>("frequency-limit-note"),
  frequencyTableHead: getElement<HTMLTableSectionElement>("frequency-table-head"),
  frequencyTableBody: getElement<HTMLTableSectionElement>("frequency-table-body"),
  frequencyFilesModal: getElement<HTMLDivElement>("frequency-files-modal"),
  frequencyFilesCloseButton: getElement<HTMLButtonElement>("frequency-files-close-button"),
  frequencyFilesSearchInput: getElement<HTMLInputElement>("frequency-files-search-input"),
  frequencyFilesSelectAllButton: getElement<HTMLButtonElement>("frequency-files-select-all"),
  frequencyFilesClearAllButton: getElement<HTMLButtonElement>("frequency-files-clear-all"),
  frequencyFilesList: getElement<HTMLDivElement>("frequency-files-list"),
  frequencyFilesStatus: getElement<HTMLSpanElement>("frequency-files-status"),
  frequencyFilesCancelButton: getElement<HTMLButtonElement>("frequency-files-cancel"),
  frequencyFilesApplyButton: getElement<HTMLButtonElement>("frequency-files-apply"),
  projectContext: getElement<HTMLDivElement>("project-context"),
  projectContextName: getElement<HTMLSpanElement>("project-context-name"),
  projectContextPath: getElement<HTMLSpanElement>("project-context-path"),
  menuFileDropdown: getElement<HTMLDivElement>("menu-file-dropdown"),
  menuProcessDropdown: getElement<HTMLDivElement>("menu-process-dropdown"),
  menuNewProject: getElement<HTMLDivElement>("menu-new-project"),
  menuOpenProject: getElement<HTMLDivElement>("menu-open-project"),
  menuOpenFolderProject: getElement<HTMLDivElement>("menu-open-folder-project"),
  menuCloseProject: getElement<HTMLDivElement>("menu-close-project"),
  btnNewProject: getElement<HTMLButtonElement>("btn-new-project"),
  btnOpenProject: getElement<HTMLButtonElement>("btn-open-project"),
  btnOpenFolderProject: getElement<HTMLButtonElement>("btn-open-folder-project"),
  welcomeStatus: getElement<HTMLDivElement>("welcome-status"),
  recentProjectsList: getElement<HTMLDivElement>("recent-projects-list"),

  // Sources
  sourceAddFilesBtn: getElement<HTMLButtonElement>("source-add-files-btn"),
  sourceAddFolderBtn: getElement<HTMLButtonElement>("source-add-folder-btn"),
  sourcePreparePdfsBtn: getElement<HTMLButtonElement>("source-prepare-pdfs-btn"),
  sourceRemoveBtn: getElement<HTMLButtonElement>("source-remove-btn"),
  sourceAddDropdown: getElement<HTMLDivElement>("source-add-dropdown"),
  sourceAddTrigger: getElement<HTMLButtonElement>("source-add-trigger"),
  sourceSelectedDropdown: getElement<HTMLDivElement>("source-selected-dropdown"),
  sourceSelectedTrigger: getElement<HTMLButtonElement>("source-selected-trigger"),
  sourceActionAddFiles: getElement<HTMLDivElement>("source-action-add-files"),
  sourceActionAddFolder: getElement<HTMLDivElement>("source-action-add-folder"),
  sourceActionExtract: getElement<HTMLDivElement>("source-action-extract"),
  sourceActionReExtract: getElement<HTMLDivElement>("source-action-re-extract"),
  sourceActionRetry: getElement<HTMLDivElement>("source-action-retry"),
  sourceActionChangeHandling: getElement<HTMLDivElement>("source-action-change-handling"),
  sourceActionRemove: getElement<HTMLDivElement>("source-action-remove"),
  fileList: getElement<HTMLUListElement>("file-list"),
  searchInput: getElement<HTMLInputElement>("search-input"),
  selectAllCheckbox: getElement<HTMLInputElement>("select-all-checkbox"),
  previewCapInput: getElement<HTMLInputElement>("preview-cap-input"),
  previewCapWarning: getElement<HTMLDivElement>("preview-cap-warning"),
  filesStatus: getElement<HTMLDivElement>("files-status"),
  corpusSummary: getElement<HTMLDivElement>("corpus-summary"),
  previewContent: getElement<HTMLDivElement>("preview-content"),
  processedPreviewContent: getElement<HTMLDivElement>("processed-preview-content"),
  previewLoadingOverlay: getElement<HTMLDivElement>("preview-loading-overlay"),
  statusBar: getElement<HTMLElement>("status-bar"),
  previewTabs: document.querySelectorAll<HTMLElement>(".right-panel .tab"),
  previewTabContents: document.querySelectorAll<HTMLElement>(".right-panel .tab-content"),
  settingsTabs: document.querySelectorAll<HTMLElement>("#settings-modal .tab"),
  settingsTabContents: document.querySelectorAll<HTMLElement>("#settings-modal .tab-content"),
  chkNormalizeWhitespaceLineEndings: getElement<HTMLInputElement>("chk-normalize-whitespace-line-endings"),
  chkRepairWrappedPdfLineBreaks: getElement<HTMLInputElement>("chk-repair-wrapped-pdf-line-breaks"),
  chkRemoveIsolatedPageMarkers: getElement<HTMLInputElement>("chk-remove-isolated-page-markers"),
  chkRemoveUrlOnlyLines: getElement<HTMLInputElement>("chk-remove-url-only-lines"),
  chkRemoveEmailOnlyLines: getElement<HTMLInputElement>("chk-remove-email-only-lines"),
  chkRemoveDoiOnlyLines: getElement<HTMLInputElement>("chk-remove-doi-only-lines"),
  chkRemoveFigureTableCaptions: getElement<HTMLInputElement>("chk-remove-figure-table-captions"),
  chkLowercase: getElement<HTMLInputElement>("chk-lowercase"),
  chkNormalizeUnicode: getElement<HTMLInputElement>("chk-normalize-unicode"),
  chkReplaceDiacritics: getElement<HTMLInputElement>("chk-replace-diacritics"),
  chkExtractHtml: getElement<HTMLInputElement>("chk-extract-html"),
  chkRemoveObviousExtractionNoise: getElement<HTMLInputElement>("chk-remove-obvious-extraction-noise"),
  selTableExtraction: getElement<HTMLSelectElement>("sel-table-extraction"),
  selPdfTextSource: getElement<HTMLSelectElement>("sel-pdf-text-source"),
  selPdfOcrQuality: getElement<HTMLSelectElement>("sel-pdf-ocr-quality"),
  selPdfEmbeddedTextStrategy: getElement<HTMLSelectElement>("sel-pdf-embedded-text-strategy"),
  chkRemoveHeaders: getElement<HTMLInputElement>("chk-remove-headers"),
  chkRemoveFooters: getElement<HTMLInputElement>("chk-remove-footers"),
  chkRemoveFootnotes: getElement<HTMLInputElement>("chk-remove-footnotes"),
  chkRemoveEndnotes: getElement<HTMLInputElement>("chk-remove-endnotes"),
  chkRemoveComments: getElement<HTMLInputElement>("chk-remove-comments"),
  chkRemoveToc: getElement<HTMLInputElement>("chk-remove-toc"),
  chkRemoveRepeatedPdfHeadersFooters: getElement<HTMLInputElement>("chk-remove-repeated-pdf-headers-footers"),
  chkRemovePdfCodeLikeBlocks: getElement<HTMLInputElement>("chk-remove-pdf-code-like-blocks"),
  chkRemovePdfFormulaLikeLines: getElement<HTMLInputElement>("chk-remove-pdf-formula-like-lines"),
  menuSaveCorpus: getElement<HTMLDivElement>("menu-save-corpus"),
  menuProcessingParams: getElement<HTMLDivElement>("menu-processing-params"),
  menuAboutCorpusWright: getElement<HTMLDivElement>("menu-about-corpuswright"),
  aboutModal: getElement<HTMLDivElement>("about-modal"),
  btnCloseAboutModal: getElement<HTMLButtonElement>("btn-close-about-modal"),
  btnCloseAboutModalTop: getElement<HTMLButtonElement>("btn-close-about-modal-top"),
  pdfIntakeModal: getElement<HTMLDivElement>("pdf-intake-modal"),
  pdfIntakeTitle: getElement<HTMLHeadingElement>("pdf-intake-title"),
  pdfIntakeDescription: getElement<HTMLParagraphElement>("pdf-intake-description"),
  pdfIntakeStoredHandling: getElement<HTMLDivElement>("pdf-intake-stored-handling"),
  pdfIntakeStoredHandlingSummary: getElement<HTMLParagraphElement>("pdf-intake-stored-handling-summary"),
  pdfIntakeStoredHandlingList: getElement<HTMLUListElement>("pdf-intake-stored-handling-list"),
  pdfIntakeChangeHandlingBtn: getElement<HTMLButtonElement>("pdf-intake-change-handling-btn"),
  pdfIntakeProfiles: document.querySelector<HTMLElement>(".pdf-intake-profiles")!,
  btnClosePdfIntakeModalTop: getElement<HTMLButtonElement>("btn-close-pdf-intake-modal-top"),
  cancelPdfIntakeBtn: getElement<HTMLButtonElement>("cancel-pdf-intake-btn"),
  loadPdfIntakeFilesBtn: getElement<HTMLButtonElement>("load-pdf-intake-files-btn"),
  choosePdfIntakeFilesBtn: getElement<HTMLButtonElement>("choose-pdf-intake-files-btn"),
  pdfIntakeStatus: getElement<HTMLSpanElement>("pdf-intake-status"),
  pdfIntakeAuditPanel: getElement<HTMLDivElement>("pdf-intake-audit-panel"),
  pdfIntakeAuditSummary: getElement<HTMLParagraphElement>("pdf-intake-audit-summary"),
  pdfIntakeAuditResults: getElement<HTMLDivElement>("pdf-intake-audit-results"),
  pdfIntakeMaterializationPanel: getElement<HTMLDivElement>("pdf-intake-materialization-panel"),
  pdfIntakeMaterializationProfile: getElement<HTMLElement>("pdf-intake-materialization-profile"),
  pdfIntakeMaterializationMethod: getElement<HTMLElement>("pdf-intake-materialization-method"),
  pdfIntakeMaterializationFile: getElement<HTMLElement>("pdf-intake-materialization-file"),
  pdfIntakeMaterializationPage: getElement<HTMLElement>("pdf-intake-materialization-page"),
  pdfIntakeMaterializationElapsed: getElement<HTMLElement>("pdf-intake-materialization-elapsed"),
  pdfIntakeMaterializationRemaining: getElement<HTMLElement>("pdf-intake-materialization-remaining"),
  pdfIntakeMaterializationSpeed: getElement<HTMLElement>("pdf-intake-materialization-speed"),
  pdfIntakeMaterializationProgressFill: getElement<HTMLDivElement>("pdf-intake-materialization-progress-fill"),
  pdfIntakeMaterializationWarningRow: getElement<HTMLDivElement>("pdf-intake-materialization-warning-row"),
  pdfIntakeMaterializationWarning: getElement<HTMLElement>("pdf-intake-materialization-warning"),
  sourceImportModal: getElement<HTMLDivElement>("source-import-modal"),
  sourceImportTitle: getElement<HTMLHeadingElement>("source-import-title"),
  sourceImportDescription: getElement<HTMLParagraphElement>("source-import-description"),
  btnCloseSourceImportModalTop: getElement<HTMLButtonElement>("btn-close-source-import-modal-top"),
  sourceImportSummary: getElement<HTMLParagraphElement>("source-import-summary"),
  sourceImportGroups: getElement<HTMLDivElement>("source-import-groups"),
  sourceImportPdfHandling: getElement<HTMLElement>("source-import-pdf-handling"),
  sourceImportResetPdfHandling: getElement<HTMLButtonElement>("source-import-reset-pdf-handling"),
  sourceImportPdfScope: getElement<HTMLParagraphElement>("source-import-pdf-scope"),
  sourceImportPdfRecommendations: getElement<HTMLDivElement>("source-import-pdf-recommendations"),
  sourceImportPdfGroupControls: getElement<HTMLDivElement>("source-import-pdf-group-controls"),
  sourceImportList: getElement<HTMLDivElement>("source-import-list"),
  sourceImportOcrQualityRow: getElement<HTMLLabelElement>("source-import-ocr-quality-row"),
  sourceImportOcrQuality: getElement<HTMLSelectElement>("source-import-ocr-quality"),
  sourceImportOcrQualityHelp: getElement<HTMLSpanElement>("source-import-ocr-quality-help"),
  sourceImportStatus: getElement<HTMLSpanElement>("source-import-status"),
  cancelSourceImportBtn: getElement<HTMLButtonElement>("cancel-source-import-btn"),
  confirmSourceImportBtn: getElement<HTMLButtonElement>("confirm-source-import-btn"),
  confirmSourceImportExtractBtn: getElement<HTMLButtonElement>("confirm-source-import-extract-btn"),
  settingsModal: getElement<HTMLDivElement>("settings-modal"),
  cancelSettingsBtn: getElement<HTMLButtonElement>("cancel-settings-btn"),
  applySettingsBtn: getElement<HTMLButtonElement>("apply-settings-btn"),
  btnCloseSettingsModalTop: getElement<HTMLButtonElement>("btn-close-settings-modal-top"),
  customRemovalInput: getElement<HTMLInputElement>("custom-removal-input"),
  btnAddCustomRemoval: getElement<HTMLButtonElement>("btn-add-custom-removal"),
  btnClearCustomRemovals: getElement<HTMLButtonElement>("btn-clear-custom-removals"),
  customRemovalsList: getElement<HTMLDivElement>("custom-removals-list"),
  customRemovalsCount: getElement<HTMLSpanElement>("custom-removals-count"),
  btnFindRepeatedArtifactsSettings: getElement<HTMLButtonElement>("btn-find-repeated-artifacts-settings"),
  repeatedArtifactScopeHelp: getElement<HTMLSpanElement>("repeated-artifact-scope-help"),
  repeatedArtifactRulesSummary: getElement<HTMLParagraphElement>("repeated-artifact-rules-summary"),
  btnLoadConfig: getElement<HTMLButtonElement>("btn-load-config"),
  btnSaveConfig: getElement<HTMLButtonElement>("btn-save-config"),
  modalConfigStatus: getElement<HTMLSpanElement>("modal-config-status"),
  themeToggle: getElement<HTMLButtonElement>("theme-toggle"),
  previewSearchInput: getElement<HTMLInputElement>("preview-search-input"),
  previewMatchCount: getElement<HTMLSpanElement>("preview-match-count"),
  searchPrev: getElement<HTMLButtonElement>("search-prev"),
  searchNext: getElement<HTMLButtonElement>("search-next"),
  sidebarSplitter: getElement<HTMLDivElement>("sidebar-splitter"),
  menuRepeatedArtifactFinder: getElement<HTMLDivElement>("menu-repeated-artifact-finder"),
  repeatedArtifactModal: getElement<HTMLDivElement>("repeated-artifact-modal"),
  btnCloseArtifactModal: getElement<HTMLButtonElement>("btn-close-artifact-modal"),
  btnCloseArtifactModalTop: getElement<HTMLButtonElement>("btn-close-artifact-modal-top"),
  btnRunArtifactScan: getElement<HTMLButtonElement>("btn-run-artifact-scan"),
  btnCancelScan: getElement<HTMLButtonElement>("btn-cancel-artifact-scan"),
  tblArtifactCandidates: getElement<HTMLTableSectionElement>("tbl-artifact-candidates"),
  lblArtifactResultsCount: getElement<HTMLSpanElement>("lbl-artifact-results-count"),
  artifactDetailsContent: getElement<HTMLDivElement>("artifact-details-content"),
  lblScanTime: getElement<HTMLSpanElement>("lbl-artifact-scan-time"),
  lblArtifactScanScope: getElement<HTMLSpanElement>("lbl-artifact-scan-scope"),
  btnAddSelectedRemovals: getElement<HTMLButtonElement>("btn-add-selected-removals"),
  lblArtifactAddStatus: getElement<HTMLSpanElement>("lbl-artifact-add-status"),
  artifactProcessedWarning: getElement<HTMLDivElement>("artifact-processed-warning"),
  artifactDiagnostics: getElement<HTMLDivElement>("artifact-scan-diagnostics"),
  chkArtifactExact: getElement<HTMLInputElement>("chk-artifact-exact"),
  chkArtifactNorm: getElement<HTMLInputElement>("chk-artifact-norm"),
  chkArtifactInline: getElement<HTMLInputElement>("chk-artifact-inline"),
  chkArtifact2Line: getElement<HTMLInputElement>("chk-artifact-2line"),
  chkArtifact3Line: getElement<HTMLInputElement>("chk-artifact-3line"),
  chkArtifactText: getElement<HTMLInputElement>("chk-artifact-text"),
  chkArtifactMixed: getElement<HTMLInputElement>("chk-artifact-mixed"),
  chkArtifactNumeric: getElement<HTMLInputElement>("chk-artifact-numeric"),
  chkArtifactSymbol: getElement<HTMLInputElement>("chk-artifact-symbol"),
  numArtifactMinOcc: getElement<HTMLInputElement>("num-artifact-min-occ"),
  numArtifactMinFiles: getElement<HTMLInputElement>("num-artifact-min-files"),
  numArtifactMaxCand: getElement<HTMLInputElement>("num-artifact-max-cand"),
  numArtifactMaxExamples: getElement<HTMLInputElement>("num-artifact-max-examples"),
};

export function getFileListContainer(): HTMLElement | null {
  return document.querySelector(".file-list-container") as HTMLElement | null;
}

export function getSummaryTotalWords(): HTMLElement | null {
  return document.getElementById("summary-total-words");
}

export function getSummaryAvgWords(): HTMLElement | null {
  return document.getElementById("summary-avg-words");
}

export function getSummaryWordLabel(): HTMLElement | null {
  return document.getElementById("summary-word-label");
}

export function getActivePreviewCardsContainer(): HTMLDivElement | null {
  return document.querySelector(".right-panel .tab-content.active .preview-cards-container") as HTMLDivElement | null;
}

export function getActivePreviewTab(): HTMLElement | null {
  return document.querySelector(".right-panel .tab.active") as HTMLElement | null;
}

export function getArtifactTextModeRadios(): NodeListOf<HTMLInputElement> {
  return document.querySelectorAll('input[name="artifact-text-mode"]') as NodeListOf<HTMLInputElement>;
}

export function getSelectedArtifactTextMode(): HTMLInputElement | null {
  return document.querySelector('input[name="artifact-text-mode"]:checked') as HTMLInputElement | null;
}

export function getDropdowns(): NodeListOf<HTMLElement> {
  return document.querySelectorAll(".dropdown") as NodeListOf<HTMLElement>;
}

export function getDropdownItems(): NodeListOf<HTMLElement> {
  return document.querySelectorAll(".dropdown-item") as NodeListOf<HTMLElement>;
}

export function getLeftPanel(): HTMLElement | null {
  return document.querySelector(".left-panel") as HTMLElement | null;
}
