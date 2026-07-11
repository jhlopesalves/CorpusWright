import type { ConcordanceResult } from "../../analysis";
import {
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  concordanceRangePercent,
} from "../../concordance";
import { dom } from "../../dom";
import { formatFixedNumber, formatNumber } from "../shell";
import { refreshConcordance } from "./shared";

export const CONCORDANCE_DISTRIBUTION_PAGE_SIZE = 10;

let concordanceDistributionOffset = 0;

export function renderConcordanceDistribution(
  result: ConcordanceResult | null,
  emptyMessage = CONCORDANCE_NO_QUERY_MESSAGE,
): void {
  dom.concordanceDistribution.replaceChildren();
  const distribution = result?.distribution ?? null;

  const panel = document.createElement("div");
  panel.className = "concordance-document-spread-panel";

  const panelHeader = document.createElement("div");
  panelHeader.className = "concordance-document-spread-header";
  const title = document.createElement("span");
  title.className = "concordance-document-spread-title";
  if (result && result.totalHits > 0) {
    title.textContent = "Distribution";
  } else if (result) {
    title.textContent = "Distribution — no hits in the selected files";
  } else {
    title.textContent = `Distribution — ${emptyMessage}`;
  }
  panelHeader.appendChild(title);
  panel.appendChild(panelHeader);

  if (result && result.totalHits > 0) {
    const metrics = document.createElement("div");
    metrics.className = "concordance-distribution-metrics";

    const hitMetric = document.createElement("div");
    hitMetric.className = "concordance-distribution-metric";
    const hitLabel = document.createElement("span");
    hitLabel.className = "concordance-distribution-metric-label";
    hitLabel.textContent = "Hits";
    const hitValue = document.createElement("strong");
    hitValue.textContent = formatNumber(result.totalHits);
    hitMetric.append(hitLabel, hitValue);

    const rangeMetric = document.createElement("div");
    rangeMetric.className = "concordance-distribution-metric";
    const rangeLabel = document.createElement("span");
    rangeLabel.className = "concordance-distribution-metric-label";
    rangeLabel.textContent = "Range";
    const rangeValue = document.createElement("strong");
    rangeValue.textContent =
      `${formatNumber(result.documentsWithHits)} / ${formatNumber(result.selectedDocumentsTotal)} files · ${formatFixedNumber(concordanceRangePercent(result), 1)}%`;
    rangeMetric.append(rangeLabel, rangeValue);

    const topFileMetric = document.createElement("div");
    topFileMetric.className = "concordance-distribution-metric";
    const topFileLabel = document.createElement("span");
    topFileLabel.className = "concordance-distribution-metric-label";
    topFileLabel.textContent = "Top file";
    const topFileValue = document.createElement("strong");
    topFileValue.textContent = `${formatFixedNumber(result.topDocumentShare * 100, 1)}%`;
    topFileMetric.append(topFileLabel, topFileValue);

    const dispersionMetric = document.createElement("div");
    dispersionMetric.className = "concordance-distribution-metric";
    const dispersionLabel = document.createElement("span");
    dispersionLabel.className = "concordance-distribution-metric-label";
    dispersionLabel.textContent = "Dispersion";
    const dispersionValue = document.createElement("strong");
    dispersionValue.textContent = `DP ${formatFixedNumber(result.dp, 2)} · ${result.dispersionLabel}`;
    dispersionMetric.append(dispersionLabel, dispersionValue);

    metrics.append(hitMetric, rangeMetric, topFileMetric, dispersionMetric);
    panel.appendChild(metrics);
  }

  const tableWrap = document.createElement("div");
  tableWrap.className = "concordance-distribution-table-wrap";

  const table = document.createElement("table");
  table.className = "concordance-distribution-table";

  const head = document.createElement("thead");
  const header = document.createElement("tr");
  for (const label of ["File", "Hits", "Tokens", "Hits per 10k", "Share of hits"]) {
    const cell = document.createElement("th");
    cell.textContent = label;
    header.appendChild(cell);
  }
  head.appendChild(header);
  table.appendChild(head);

  const body = document.createElement("tbody");
  if (!result || result.totalHits === 0 || distribution === null) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "frequency-empty concordance-empty";
    cell.textContent = !result
      ? emptyMessage
      : distribution === null
        ? "Distribution was not included in this response."
        : CONCORDANCE_NO_HITS_MESSAGE;
    row.appendChild(cell);
    body.appendChild(row);
  } else {
    for (const item of distribution.items) {
      const row = document.createElement("tr");

      const file = document.createElement("td");
      file.className = "concordance-distribution-file";
      file.title = item.displayName || item.sourcePath;
      file.textContent = item.displayName || item.sourcePath;

      const count = document.createElement("td");
      count.className = "numeric";
      count.textContent = formatNumber(item.count);

      const tokens = document.createElement("td");
      tokens.className = "numeric";
      tokens.textContent = formatNumber(item.tokenCount);

      const rate = document.createElement("td");
      rate.className = "numeric";
      rate.textContent = formatFixedNumber(item.ratePer10k, 2);

      const share = document.createElement("td");
      share.className = "numeric";
      const shareOfHits = result.totalHits > 0 ? item.count / result.totalHits * 100 : 0;
      share.textContent = `${formatFixedNumber(shareOfHits, 1)}%`;

      row.append(file, count, tokens, rate, share);
      body.appendChild(row);
    }
  }

  table.appendChild(body);
  tableWrap.appendChild(table);
  panel.appendChild(tableWrap);

  if (result && result.totalHits > 0 && distribution) {
    const pagination = document.createElement("div");
    pagination.className = "analysis-pagination";
    const previous = document.createElement("button");
    previous.type = "button";
    previous.className = "secondary-btn";
    previous.textContent = "Previous";
    previous.disabled = distribution.offset === 0;
    previous.addEventListener("click", () => {
      concordanceDistributionOffset = Math.max(0, distribution.offset - distribution.limit);
      void refreshConcordance(false, false, false);
    });
    const status = document.createElement("span");
    const first = distribution.totalDocuments === 0 ? 0 : distribution.offset + 1;
    const last = Math.min(
      distribution.totalDocuments,
      distribution.offset + distribution.items.length,
    );
    status.textContent = `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(distribution.totalDocuments)}`;
    const next = document.createElement("button");
    next.type = "button";
    next.className = "secondary-btn";
    next.textContent = "Next";
    next.disabled = distribution.offset + distribution.items.length >= distribution.totalDocuments;
    next.addEventListener("click", () => {
      concordanceDistributionOffset = distribution.offset + distribution.limit;
      void refreshConcordance(false, false, false);
    });
    pagination.append(previous, status, next);
    panel.appendChild(pagination);
  }
  dom.concordanceDistribution.appendChild(panel);
}

export function currentConcordanceDistributionOffset(): number {
  return concordanceDistributionOffset;
}

export function resetConcordanceDistributionOffset(): void {
  concordanceDistributionOffset = 0;
}
