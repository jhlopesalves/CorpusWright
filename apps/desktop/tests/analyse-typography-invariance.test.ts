import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

function ruleBody(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return styles.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

for (const selector of [
  ".analyse-workbench",
  ".analyse-content",
  ".analyse-status-row",
  ".analyse-workspace-shell",
  ".analyse-tool-content",
  ".analyse-overview",
]) {
  assert.doesNotMatch(
    ruleBody(selector),
    /(?:^|;)\s*(?:font(?:-family|-weight|-style|-stretch|-synthesis|-variation-settings)?|letter-spacing|text-shadow|transform|zoom)\s*:/,
    `${selector} must not leak typography into Analyse state content`,
  );
}

assert.match(
  ruleBody(".analyse-workbench"),
  /scrollbar-gutter:\s*stable/,
  "Analyse must reserve its scrollbar gutter so state height cannot shift text onto half pixels",
);

const stableMarkup = html.slice(
  html.indexOf('<section id="analyse-workbench"'),
  html.indexOf('<section id="frequency-list-pane"'),
);
for (const witness of [
  'class="analyse-eyebrow">Analyse',
  '<h1>Analyse Corpus</h1>',
  'id="analyse-tools-title">Analysis tools',
  'id="analyse-overview-title">Corpus Overview',
  'id="analyse-build-button" class="primary-btn"',
  'id="analyse-build-note"',
]) {
  assert(stableMarkup.includes(witness), `Stable Analyse markup must retain ${witness}`);
}

const renderAnalysisTools = shell.match(
  /function renderAnalysisTools\(\): void \{([\s\S]*?)\n\}\n\nfunction renderAnalysisToolPanels/,
)?.[1] ?? "";
assert.match(renderAnalysisTools, /button\.className = "analyse-tool-item"/);
assert.match(renderAnalysisTools, /button\.dataset\.analysisTool = tool\.id/);
assert.match(renderAnalysisTools, /button\.textContent = tool\.name/);
assert.doesNotMatch(
  renderAnalysisTools,
  /analysisStatus|analysisCorpus|is-ready|is-building|fontWeight|style\.font/,
  "Stable Analyse navigation must not vary its classes or typography by analysis state",
);

const statefulSelector = /([^{}]*(?:analysis|analyse)[^{}]*(?:ready|building|cancelled|error|stale|not-built|result-state)[^{}]*)\{([^{}]*)\}/gi;
for (const match of styles.matchAll(statefulSelector)) {
  assert.doesNotMatch(
    match[2]!,
    /font(?:-family|-weight|-style|-stretch|-synthesis|-variation-settings)?\s*:/,
    `Analysis state selector must not alter inherited typography: ${match[1]!.trim()}`,
  );
}

for (const selector of [
  ".stage-tab-title",
  ".stage-tab.active .stage-tab-title",
  ".analyse-eyebrow",
  ".analyse-header h1",
  ".analyse-status-text",
  ".analyse-section h2",
  ".analyse-tool-rail h2",
  '.analyse-tool-item[aria-current="page"]',
]) {
  const body = ruleBody(selector);
  assert.match(body, /font-weight:\s*700/);
  assert.doesNotMatch(
    body,
    /font-weight:\s*(?:[1-9]50)/,
    `${selector} must use an available face rather than a synthetic intermediate weight`,
  );
}
