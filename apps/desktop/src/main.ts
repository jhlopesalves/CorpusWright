import "./state";
import "./dom";

import { initCorpusHandlers } from "./corpus";
import { initCustomRemovals, renderCustomRemovals } from "./custom-removals";
import { initExport } from "./export";
import {
  configureFileListCallbacks,
  initFileFilter,
  initSelectionControls,
  initVirtualScroll,
} from "./file-list";
import {
  initPreviewTabs,
  invalidatePreviewSession,
  schedulePreviewUpdate,
  updatePreview,
} from "./preview";
import { initRepeatedArtifactFinder } from "./repeated-artifacts";
import { handlePreviewTabChanged, initSearch } from "./search";
import { initSettingsModal, syncInitialSettingsControls } from "./settings-modal";
import { initAppChrome, initThemeToggle } from "./theme";
import { updateWordCount } from "./word-count";

function init(): void {
  syncInitialSettingsControls();

  initCorpusHandlers({
    updateWordCount,
  });

  initExport();

  configureFileListCallbacks({
    updatePreview,
    schedulePreviewUpdate,
    invalidatePreviewSession,
  });
  initVirtualScroll();

  initThemeToggle();

  initCustomRemovals();

  initSearch({
    schedulePreviewUpdate,
  });

  initFileFilter();

  initPreviewTabs({
    onPreviewTabChanged: handlePreviewTabChanged,
  });

  initSettingsModal({
    updateWordCount,
    schedulePreviewUpdate,
  });

  initSelectionControls();

  initAppChrome();

  initRepeatedArtifactFinder({
    renderCustomRemovals,
    schedulePreviewUpdate,
  });
}

document.addEventListener("DOMContentLoaded", init);
