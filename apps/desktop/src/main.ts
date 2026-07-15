/**
 * Application composition root.
 *
 * Initialisation order establishes shared state and DOM references before feature controllers
 * register their callbacks. Feature modules own subsequent interaction lifecycles.
 */
import "./app/state";
import "./app/dom";

import { initAboutModal } from "./app/about-modal";
import { initAppearanceDialog } from "./app/appearance";
import { initCorpusHandlers } from "./features/prepare/corpus/controller";
import { initCustomRemovals, renderCustomRemovals } from "./features/prepare/cleaning/custom-removals";
import { initExport } from "./features/prepare/export";
import {
  configureFileListCallbacks,
  initFileFilter,
  initSelectionControls,
  initVirtualScroll,
} from "./features/prepare/corpus/file-list";
import {
  initPreviewTabs,
  invalidatePreviewSession,
  schedulePreviewUpdate,
  updatePreview,
} from "./features/prepare/preview/controller";
import { initProjectEntry } from "./features/project/controller";
import { initRepeatedArtifactFinder } from "./features/prepare/repeated-artifacts/controller";
import { handlePreviewTabChanged, initSearch } from "./features/prepare/search";
import { initSettingsModal, syncInitialSettingsControls } from "./app/settings-modal";
import { initAppChrome, initThemeToggle } from "./app/theme";
import { initWorkbenchSwitcher } from "./features/analyse/workbench";
import { updateWordCount } from "./features/prepare/corpus/word-count";

function init(): void {
  syncInitialSettingsControls();

  initCorpusHandlers({
    updateWordCount,
  });

  initProjectEntry();

  initExport();

  configureFileListCallbacks({
    updatePreview,
    schedulePreviewUpdate,
    invalidatePreviewSession,
  });
  initVirtualScroll();

  initAppearanceDialog();

  initThemeToggle();

  initWorkbenchSwitcher();

  initCustomRemovals();

  initAboutModal();

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
    updateWordCount,
  });
}

document.addEventListener("DOMContentLoaded", init);

if (import.meta.env.VITE_BROWSER_WORKFLOW === "1"
  && new URLSearchParams(window.location.search).has("browser-workflow")) {
  void import("./testing/browser-workflow").then(({ installBrowserWorkflowHarness }) => {
    installBrowserWorkflowHarness();
  });
}
