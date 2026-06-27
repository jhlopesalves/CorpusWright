export const CONCORDANCE_MIN_WINDOW = 1;
export const CONCORDANCE_DEFAULT_WINDOW = 5;
export const CONCORDANCE_MAX_WINDOW = 20;

export function normaliseConcordanceQuery(value: string): string {
  return value.trim();
}

export function clampConcordanceWindow(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return CONCORDANCE_DEFAULT_WINDOW;
  }
  return Math.min(CONCORDANCE_MAX_WINDOW, Math.max(CONCORDANCE_MIN_WINDOW, Math.trunc(value)));
}

export function concordanceSummaryText(result: {
  totalHits: number;
  documentsWithHits: number;
  windowTokens: number;
}): string {
  return `${result.totalHits.toLocaleString()} hits in ${result.documentsWithHits.toLocaleString()} files · window ${result.windowTokens.toLocaleString()}`;
}
