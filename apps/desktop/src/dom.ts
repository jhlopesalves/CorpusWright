function getElement<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

export const dom = {
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
  chkJoinLineBreaks: getElement<HTMLInputElement>("chk-join-line-breaks"),
  chkNormalizeIrregularLineBreaks: getElement<HTMLInputElement>("chk-normalize-irregular-line-breaks"),
  chkRemoveStandalonePageNumbers: getElement<HTMLInputElement>("chk-remove-standalone-page-numbers"),
  chkRemoveStandaloneRomanPageNumbers: getElement<HTMLInputElement>("chk-remove-standalone-roman-page-numbers"),
  chkRemovePageIndicators: getElement<HTMLInputElement>("chk-remove-page-indicators"),
  chkRemovePageDelimiters: getElement<HTMLInputElement>("chk-remove-page-delimiters"),
  chkLowercase: getElement<HTMLInputElement>("chk-lowercase"),
  chkNormalize: getElement<HTMLInputElement>("chk-normalize"),
  chkTrim: getElement<HTMLInputElement>("chk-trim"),
  chkCollapse: getElement<HTMLInputElement>("chk-collapse"),
  chkNormalizeUnicode: getElement<HTMLInputElement>("chk-normalize-unicode"),
  chkReplaceDiacritics: getElement<HTMLInputElement>("chk-replace-diacritics"),
  chkExtractHtml: getElement<HTMLInputElement>("chk-extract-html"),
  selTableExtraction: getElement<HTMLSelectElement>("sel-table-extraction"),
  selPdfEmbeddedTextStrategy: getElement<HTMLSelectElement>("sel-pdf-embedded-text-strategy"),
  chkRemoveHeaders: getElement<HTMLInputElement>("chk-remove-headers"),
  chkRemoveFooters: getElement<HTMLInputElement>("chk-remove-footers"),
  chkRemoveFootnotes: getElement<HTMLInputElement>("chk-remove-footnotes"),
  chkRemoveEndnotes: getElement<HTMLInputElement>("chk-remove-endnotes"),
  chkRemoveComments: getElement<HTMLInputElement>("chk-remove-comments"),
  chkRemoveToc: getElement<HTMLInputElement>("chk-remove-toc"),
  chkRemoveRepeatedPdfHeadersFooters: getElement<HTMLInputElement>("chk-remove-repeated-pdf-headers-footers"),
  chkRemovePdfPageLabels: getElement<HTMLInputElement>("chk-remove-pdf-page-labels"),
  chkRemovePdfSymbolHeavyArtifacts: getElement<HTMLInputElement>("chk-remove-pdf-symbol-heavy-artifacts"),
  chkRemovePdfCodeLikeBlocks: getElement<HTMLInputElement>("chk-remove-pdf-code-like-blocks"),
  chkRemovePdfFormulaLikeLines: getElement<HTMLInputElement>("chk-remove-pdf-formula-like-lines"),
  menuOpenDir: getElement<HTMLDivElement>("menu-open-dir"),
  menuOpenFiles: getElement<HTMLDivElement>("menu-open-files"),
  menuSaveCorpus: getElement<HTMLDivElement>("menu-save-corpus"),
  menuProcessingParams: getElement<HTMLDivElement>("menu-processing-params"),
  menuAboutCorpusWright: getElement<HTMLDivElement>("menu-about-corpuswright"),
  aboutModal: getElement<HTMLDivElement>("about-modal"),
  btnCloseAboutModal: getElement<HTMLButtonElement>("btn-close-about-modal"),
  btnCloseAboutModalTop: getElement<HTMLButtonElement>("btn-close-about-modal-top"),
  settingsModal: getElement<HTMLDivElement>("settings-modal"),
  cancelSettingsBtn: getElement<HTMLButtonElement>("cancel-settings-btn"),
  applySettingsBtn: getElement<HTMLButtonElement>("apply-settings-btn"),
  btnCloseSettingsModalTop: getElement<HTMLButtonElement>("btn-close-settings-modal-top"),
  customRemovalInput: getElement<HTMLInputElement>("custom-removal-input"),
  btnAddCustomRemoval: getElement<HTMLButtonElement>("btn-add-custom-removal"),
  btnClearCustomRemovals: getElement<HTMLButtonElement>("btn-clear-custom-removals"),
  customRemovalsList: getElement<HTMLDivElement>("custom-removals-list"),
  customRemovalsCount: getElement<HTMLSpanElement>("custom-removals-count"),
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
