import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const frequency = readFileSync(
  new URL("../src/workbench/frequency.ts", import.meta.url),
  "utf8",
);
const concordance = readFileSync(new URL("../src/concordance.ts", import.meta.url), "utf8");
const lines = readFileSync(
  new URL("../src/workbench/concordance/lines.ts", import.meta.url),
  "utf8",
);
const collocations = readFileSync(
  new URL("../src/workbench/concordance/collocations.ts", import.meta.url),
  "utf8",
);
const distribution = readFileSync(
  new URL("../src/workbench/concordance/distribution.ts", import.meta.url),
  "utf8",
);
const concordanceState = readFileSync(
  new URL("../src/workbench/concordance/state.ts", import.meta.url),
  "utf8",
);
const concordanceShared = readFileSync(
  new URL("../src/workbench/concordance/shared.ts", import.meta.url),
  "utf8",
);

const resultTableTokens = [
  "--cw-result-table-surface",
  "--cw-result-table-header-surface",
  "--cw-result-table-row-surface",
  "--cw-result-table-row-alt-surface",
  "--cw-result-table-border",
  "--cw-result-table-separator",
  "--cw-result-table-hover-surface",
  "--cw-result-table-active-surface",
  "--cw-result-table-header-text",
  "--cw-result-table-body-text",
  "--cw-result-table-muted-text",
  "--cw-result-table-track",
  "--cw-result-table-bar",
] as const;
const expectedThemes = [
  "corpus-dark",
  "corpus-light",
  "workbench-light",
  "workbench-dark",
  "retro-light",
  "retro-dark",
  "graphite-light",
  "graphite-dark",
  "solar-light",
  "solar-dark",
  "sepia-light",
  "sepia-dark",
  "moss-light",
  "moss-dark",
  "high-contrast-light",
  "high-contrast-dark",
] as const;

const themeBlocks = new Map<string, string>();
const themePattern = /(?:^|\n)\[data-theme="([^"]+)"\]\s*\{([\s\S]*?)\n\}/g;
for (const match of styles.matchAll(themePattern)) {
  themeBlocks.set(match[1]!, match[2]!);
}

for (const theme of expectedThemes) {
  const body = themeBlocks.get(theme);
  assert(body, `${theme} must have a standalone theme block`);
  for (const token of resultTableTokens) {
    assert.match(body, new RegExp(`(?:^|\\n)\\s*${token}:`), `${theme} must define ${token}`);
  }
}

function ruleBody(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return styles.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

const tableRule = ruleBody(".cw-result-table");
assert.match(tableRule, /font-family:\s*var\(--cw-font-ui\)/);
assert.match(tableRule, /font-synthesis:\s*none/);

const numericRule = ruleBody(".cw-result-table-numeric");
assert.match(numericRule, /font-variant-numeric:\s*tabular-nums/);
assert.match(numericRule, /font-weight:\s*400/);

const noteRule = ruleBody(".cw-result-table-pagination-status");
assert.match(noteRule, /color:\s*var\(--cw-result-table-muted-text\)/);
assert.match(noteRule, /font-weight:\s*400/);
assert.match(noteRule, /font-family:\s*var\(--cw-font-ui\)/);

const statefulAncestorRule = /([^{}]*(?:analysis|analyse)[^{}]*(?:ready|building|build-complete|result-state)[^{}]*)\{([^{}]*)\}/gi;
for (const match of styles.matchAll(statefulAncestorRule)) {
  assert.doesNotMatch(
    match[2]!,
    /font-(?:family|weight|style|stretch|synthesis)\s*:/,
    `Analysis state selector must not change inherited typography: ${match[1]!.trim()}`,
  );
}

assert.match(
  html,
  /class="frequency-table-wrap cw-result-table-wrap">\s*<table class="frequency-table cw-result-table">/,
  "Frequency List must consume the shared table shell",
);
assert.match(
  html,
  /id="frequency-page-status" class="cw-result-table-pagination-status"/,
  "Frequency pagination status must use shared muted typography",
);
assert(frequency.includes("frequency-token-cell frequency-col-term cw-result-table-lexical"));
assert(frequency.includes("numeric cw-result-table-numeric"));
assert(frequency.includes('track.setAttribute("aria-label", accessibilityLabel)'));
assert(frequency.includes("DP ${formatFixedNumber(dispersion, 2)} on a scale from 0 to 1"));
assert(!frequency.includes("fill.dataset.level"), "DP magnitude must not imply undocumented categories");

const frequencyTokenRule = ruleBody(".frequency-token-cell");
assert.match(frequencyTokenRule, /font-family:\s*var\(--cw-font-ui\)/);
assert.match(frequencyTokenRule, /font-weight:\s*600/);

const frequencyNumericRule = ruleBody(".frequency-table td.numeric");
assert.match(frequencyNumericRule, /font-variant-numeric:\s*tabular-nums/);
assert.match(frequencyNumericRule, /font-weight:\s*400/);

const dpTrackRule = ruleBody(".frequency-distribution-track");
assert.match(dpTrackRule, /height:\s*6px/);
assert.match(dpTrackRule, /background:\s*var\(--cw-result-table-track\)/);
assert(!styles.includes('.frequency-dp-fill[data-level='));

for (const witness of [
  'class="concordance-table-wrap cw-result-table-wrap"',
  'class="concordance-table cw-result-table"',
  'class="concordance-collocations-table-wrap cw-result-table-wrap"',
  'class="concordance-collocations-table cw-result-table"',
]) {
  assert(html.includes(witness), `${witness} must consume the shared table system`);
}
assert(distribution.includes('tableWrap.className = "concordance-distribution-table-wrap cw-result-table-wrap"'));
assert(distribution.includes('table.className = "concordance-distribution-table cw-result-table"'));

const headerRule = ruleBody(".cw-result-table th");
assert.match(headerRule, /font-family:\s*var\(--cw-font-ui\)/);
assert.match(headerRule, /font-weight:\s*500/);
assert.match(headerRule, /background:\s*var\(--cw-result-table-header-surface\)/);

const lexicalRoleUses = [frequency, lines, collocations, distribution]
  .flatMap((source) => source.match(/cw-result-table-lexical/g) ?? []);
assert.equal(lexicalRoleUses.length, 2, "Only Frequency token and Collocate should use the lexical role");
assert(collocations.includes("concordance-collocate-text cw-result-table-lexical"));

for (const numericWitness of [
  "numeric cw-result-table-numeric",
  "numeric cw-result-table-numeric concordance-collocation-score",
]) {
  assert(collocations.includes(numericWitness), `Collocates must use regular numeric role: ${numericWitness}`);
}
for (const numericWitness of [
  "numeric cw-result-table-numeric concordance-distribution-occurrences",
  "numeric cw-result-table-numeric concordance-distribution-rate",
  'share.className = "numeric cw-result-table-numeric"',
]) {
  assert(distribution.includes(numericWitness), `Distribution must use regular numeric role: ${numericWitness}`);
}

assert(lines.includes("concordance-left cw-result-table-kwic"));
assert(lines.includes("concordance-node-cell cw-result-table-kwic"));
assert(lines.includes("concordance-right cw-result-table-kwic"));
const kwicRule = ruleBody(".cw-result-table-kwic");
assert.match(kwicRule, /font-family:\s*var\(--cw-font-mono\)/);
assert.match(kwicRule, /font-weight:\s*400/);
assert.doesNotMatch(
  styles.slice(styles.indexOf(".concordance-collocation-controls"), styles.indexOf(".frequency-table-wrap")),
  /font-family:\s*var\(--cw-font-mono\)/,
  "Collocates and Distribution must remain in the UI sans stack",
);

assert(concordance.includes("export const CONCORDANCE_DEFAULT_PAGE_SIZE = 24;"));
assert.match(
  html,
  /id="concordance-page-size-input" type="number" min="1" max="500" step="1" value="24"/,
);
assert(concordanceState.includes("pageSize: CONCORDANCE_DEFAULT_PAGE_SIZE"));
assert(lines.includes("dom.concordancePageSizeInput.value = pageSize.toString()"));
assert(concordanceShared.includes("limit: workbenchState.lines.pageSize"));

for (const paginationWitness of [
  'id="frequency-page-status" class="cw-result-table-pagination-status"',
  'id="concordance-page-status" class="cw-result-table-pagination-status"',
  'id="concordance-collocations-more-status" class="cw-result-table-pagination-status"',
]) {
  assert(html.includes(paginationWitness), `${paginationWitness} must use shared status typography`);
}
assert(distribution.includes('status.className = "cw-result-table-pagination-status"'));
