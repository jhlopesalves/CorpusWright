import { invoke } from "@tauri-apps/api/core";

import { dom, getSummaryAvgWords, getSummaryTotalWords, getSummaryWordLabel } from "./dom";
import { state } from "./state";

const OCR_WORD_COUNT_SKIPPED_STATUS = "Token count skipped for OCR mode until OCR text is generated.";

type WordCountBatchResult = {
  totalWords: number;
  skippedOcrMode: boolean;
};

export async function updateWordCount(): Promise<void> {
  const myWordCountGeneration = ++state.wordCountGeneration;
  const myVersion = state.currentCorpusVersion;
  const totalWordsEl = getSummaryTotalWords();
  const avgWordsEl = getSummaryAvgWords();
  const wordLabelEl = getSummaryWordLabel();
  if (!totalWordsEl || !avgWordsEl) return;

  if (state.allFiles.length === 0) {
    totalWordsEl.textContent = "0";
    avgWordsEl.textContent = "0";
    return;
  }

  const BATCH_SIZE = 500;
  let totalWords = 0;
  const totalFiles = state.allFiles.length;

  totalWordsEl.textContent = "Counting...";
  avgWordsEl.textContent = "Counting...";
  if (wordLabelEl) wordLabelEl.textContent = "Known Cleaned Tokens";

  try {
    for (let offset = 0; offset < totalFiles; offset += BATCH_SIZE) {
      const batchSize = Math.min(BATCH_SIZE, totalFiles - offset);
      const batchIndices = Array.from({ length: batchSize }, (_, batchIndex) => offset + batchIndex);
      const batchResult = await invoke<WordCountBatchResult>("compute_word_count_command", {
        indices: batchIndices,
        corpusVersion: myVersion,
        cleaningConfig: state.activeCleaningConfig
      });
      if (batchResult.skippedOcrMode) {
        if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
        totalWordsEl.textContent = "Unavailable";
        avgWordsEl.textContent = "Unavailable";
        if (wordLabelEl) {
          wordLabelEl.textContent = "Cleaned Token Count";
          wordLabelEl.title = OCR_WORD_COUNT_SKIPPED_STATUS;
        }
        dom.statusBar.textContent = OCR_WORD_COUNT_SKIPPED_STATUS;
        return;
      }
      totalWords += batchResult.totalWords;

      const processed = Math.min(offset + BATCH_SIZE, totalFiles);
      const avgWords = totalWords / processed;
      if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
      totalWordsEl.textContent = `${totalWords.toLocaleString()} (${processed}/${totalFiles} files)`;
      avgWordsEl.textContent = avgWords.toLocaleString(undefined, { maximumFractionDigits: 2 });
    }

    if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
    if (wordLabelEl) {
      wordLabelEl.textContent = "Cleaned Token Count";
      wordLabelEl.title = "Counts whitespace-separated tokens after cleaning. OCR-mode PDFs use matching cached OCR text only.";
    }
    const avgWords = totalWords / totalFiles;
    totalWordsEl.textContent = totalWords.toLocaleString();
    avgWordsEl.textContent = avgWords.toLocaleString(undefined, { maximumFractionDigits: 2 });
  } catch (err) {
    if (myVersion !== state.currentCorpusVersion || myWordCountGeneration !== state.wordCountGeneration) return;
    totalWordsEl.textContent = "Error";
    avgWordsEl.textContent = "Error";
    console.error("Failed to compute word count", err);
  }
}
