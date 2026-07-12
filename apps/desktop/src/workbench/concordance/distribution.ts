import type { ConcordanceResult } from "../../analysis";
import {
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  concordanceDistributionMagnitudePercent,
  concordanceRangePercent,
} from "../../concordance";
import { dom } from "../../dom";
import { formatFixedNumber, formatNumber } from "../shell";
import { refreshConcordanceDistribution } from "./shared";

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
    const summary = document.createElement("p");
    summary.className = "concordance-distribution-summary";
    const primary = document.createElement("strong");
    primary.textContent = `${formatNumber(result.totalHits)} hits`;
    summary.append(
      primary,
      document.createTextNode(
        ` across ${formatNumber(result.documentsWithHits)} of ${formatNumber(result.selectedDocumentsTotal)} documents (${formatFixedNumber(concordanceRangePercent(result), 1)}%). DP = ${formatFixedNumber(result.dp, 2)}; the leading document contains ${formatFixedNumber(result.topDocumentShare * 100, 1)}% of all hits.`,
      ),
    );
    panel.appendChild(summary);

    const reading = document.createElement("p");
    reading.className = "concordance-distribution-reading cw-result-table-note";
    reading.textContent =
      "Concentration in a few documents may suggest topic, genre, or source dependence; broad reach may suggest more corpus-general use.";
    panel.appendChild(reading);
  }

  const tableWrap = document.createElement("div");
  tableWrap.className = "concordance-distribution-table-wrap cw-result-table-wrap";

  const table = document.createElement("table");
  table.className = "concordance-distribution-table cw-result-table";
  const columns = document.createElement("colgroup");
  for (const className of [
    "concordance-distribution-file-column",
    "concordance-distribution-hits-column",
    "concordance-distribution-tokens-column",
    "concordance-distribution-rate-column",
    "concordance-distribution-share-column",
  ]) {
    const column = document.createElement("col");
    column.className = className;
    columns.appendChild(column);
  }
  table.appendChild(columns);

  const head = document.createElement("thead");
  const header = document.createElement("tr");
  const headerLabels = [
    { label: "File" },
    { label: "Hits" },
    { label: "Document tokens" },
    { label: "Hits per 10k", note: "bar = rate across hit-bearing documents" },
    { label: "Share of hits" },
  ];
  for (const { label, note } of headerLabels) {
    const cell = document.createElement("th");
    const heading = document.createElement("span");
    heading.className = "concordance-distribution-heading";
    const primary = document.createElement("span");
    primary.className = "concordance-distribution-heading-primary";
    primary.textContent = label;
    heading.appendChild(primary);
    if (note) {
      const sublabel = document.createElement("span");
      sublabel.className = "concordance-distribution-heading-note";
      sublabel.textContent = note;
      heading.appendChild(sublabel);
    }
    cell.appendChild(heading);
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
      row.className = "concordance-distribution-row";

      const file = document.createElement("td");
      file.className = "concordance-distribution-file cw-result-table-file";
      file.title = item.displayName || item.sourcePath;
      file.textContent = item.displayName || item.sourcePath;

      const count = document.createElement("td");
      count.className = "numeric cw-result-table-numeric concordance-distribution-occurrences";
      count.textContent = formatNumber(item.count);

      const tokens = document.createElement("td");
      tokens.className = "numeric cw-result-table-numeric";
      tokens.textContent = formatNumber(item.tokenCount);

      const rate = document.createElement("td");
      rate.className = "numeric cw-result-table-numeric concordance-distribution-rate";
      const rateValue = document.createElement("span");
      rateValue.className = "concordance-distribution-rate-value";
      rateValue.textContent = formatFixedNumber(item.ratePer10k, 2);
      const rateDescription = `${rateValue.textContent} hits per 10,000 document tokens; bar scaled to the highest rate across all hit-bearing documents`;
      rate.title = rateDescription;
      rate.setAttribute("aria-label", rateDescription);
      const rateTrack = document.createElement("span");
      rateTrack.className = "concordance-distribution-rate-track";
      rateTrack.setAttribute("aria-hidden", "true");
      const rateBar = document.createElement("span");
      rateBar.className = "concordance-distribution-rate-bar";
      rateBar.style.setProperty(
        "--distribution-rate-width",
        `${concordanceDistributionMagnitudePercent(item.ratePer10k, distribution.maxRatePer10k)}%`,
      );
      rateTrack.appendChild(rateBar);
      rate.append(rateValue, rateTrack);

      const share = document.createElement("td");
      share.className = "numeric cw-result-table-numeric";
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
    pagination.className = "analysis-pagination cw-result-table-pagination";
    const previous = document.createElement("button");
    previous.type = "button";
    previous.className = "secondary-btn";
    previous.textContent = "Previous";
    previous.disabled = distribution.offset === 0;
    previous.addEventListener("click", () => {
      concordanceDistributionOffset = Math.max(0, distribution.offset - distribution.limit);
      void refreshConcordanceDistribution();
    });
    const status = document.createElement("span");
    status.className = "cw-result-table-pagination-status";
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
      void refreshConcordanceDistribution();
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
