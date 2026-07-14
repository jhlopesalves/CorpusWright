import { invoke } from "../ipc";

import { dom, getSummaryAvgWords, getSummaryTotalWords, getSummaryWordLabel } from "../../../app/dom";
import { state } from "../../../app/state";
import { effectiveCleaningConfig } from "../cleaning/effective-config";
import { refreshAnalyseWorkbench } from "../../analyse/workbench";

const OCR_WORD_COUNT_SKIPPED_STATUS =
  "Cleaned token count unavailable until sources are processed with cleaning settings.";

type WordCountBatchResult = {
  totalWords: number;
  skippedOcrMode: boolean;
  processedCount: number;
  totalCount: number;
};

export async function updateWordCount(): Promise<void> {
  const myWordCountGeneration = ++state.wordCountGeneration;
  const myVersion = state.currentCorpusVersion;
  const totalWordsEl = getSummaryTotalWords();
  const avgWordsEl = getSummaryAvgWords();
  const wordLabelEl = getSummaryWordLabel();
  if (!totalWordsEl || !avgWordsEl) return;

  const totalFiles = state.activeProject ? state.projectCorpusFileCount : state.allFiles.length;
  if (totalFiles === 0) {
    totalWordsEl.textContent = "0";
    avgWordsEl.textContent = "0";
    state.cleanedTokenCountText = "0";
    refreshAnalyseWorkbench();
    return;
  }

  const BATCH_SIZE = 500;
  let totalWords = 0;
  const countLabel = state.activeProject ? "Source Token Count" : "Cleaned Token Count";
  const unavailableStatus = state.activeProject
    ? "Source token count unavailable until OCR sources have materialised text."
    : OCR_WORD_COUNT_SKIPPED_STATUS;

  totalWordsEl.textContent = "Counting...";
  avgWordsEl.textContent = "Counting...";
  state.cleanedTokenCountText = "Counting...";
  refreshAnalyseWorkbench();
  if (wordLabelEl) wordLabelEl.textContent = countLabel;

  try {
    for (let offset = 0; offset < totalFiles; offset += BATCH_SIZE) {
      const batchResult = await invoke<WordCountBatchResult>("compute_word_count_window_command", {
        offset,
        limit: BATCH_SIZE,
        corpusVersion: myVersion,
        cleaningConfig: effectiveCleaningConfig()
      });
      if (batchResult.skippedOcrMode) {
        if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
        totalWordsEl.textContent = "Unavailable";
        avgWordsEl.textContent = "Unavailable";
        state.cleanedTokenCountText = "Unavailable";
        refreshAnalyseWorkbench();
        if (wordLabelEl) {
          wordLabelEl.textContent = countLabel;
          wordLabelEl.title = unavailableStatus;
        }
        dom.statusBar.textContent = unavailableStatus;
        return;
      }
      totalWords += batchResult.totalWords;

      const processed = Math.min(offset + batchResult.processedCount, batchResult.totalCount);
      const avgWords = totalWords / processed;
      if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
      totalWordsEl.textContent = `${totalWords.toLocaleString()} (${processed.toLocaleString()}/${totalFiles.toLocaleString()} files; counting)`;
      avgWordsEl.textContent = avgWords.toLocaleString(undefined, { maximumFractionDigits: 2 });
      state.cleanedTokenCountText = totalWordsEl.textContent;
      refreshAnalyseWorkbench();
      dom.statusBar.textContent = `Counting source tokens: ${processed.toLocaleString()} / ${totalFiles.toLocaleString()} files`;
    }

    if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
    if (wordLabelEl) {
      wordLabelEl.textContent = countLabel;
      wordLabelEl.title = "Counts whitespace-separated tokens after cleaning. OCR-mode PDFs use matching cached OCR text only.";
    }
    const avgWords = totalWords / totalFiles;
    totalWordsEl.textContent = totalWords.toLocaleString();
    avgWordsEl.textContent = avgWords.toLocaleString(undefined, { maximumFractionDigits: 2 });
    state.cleanedTokenCountText = totalWordsEl.textContent;
    refreshAnalyseWorkbench();
  } catch (err) {
    if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
    totalWordsEl.textContent = "Error";
    avgWordsEl.textContent = "Error";
    state.cleanedTokenCountText = "Error";
    refreshAnalyseWorkbench();
    console.error("Failed to compute word count", err);
  }
}
