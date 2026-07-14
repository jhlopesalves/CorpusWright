import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const desktopRoot = path.resolve("apps/desktop");
const graph = await loadStyleGraph(new URL("../src/styles.css", import.meta.url));
const moduleSource = (relativePath: string): string => {
  const absolutePath = path.join(desktopRoot, "src/styles", relativePath);
  const module = graph.modules.find(candidate => candidate.path === absolutePath);
  assert(module, `${relativePath} must be present in the style graph`);
  return module.contents;
};

const base = moduleSource("foundations/base.css");
const tables = moduleSource("components/tables.css");
const panels = moduleSource("components/panels.css");
const controls = moduleSource("components/controls.css");
const frequencyCss = moduleSource("features/frequency.css");
const concordanceCss = moduleSource("features/concordance.css");
const featureCss = graph.modules
  .filter(module => module.path.includes(`${path.sep}styles${path.sep}features${path.sep}`))
  .map(module => module.contents)
  .join("\n");
const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const distribution = readFileSync(new URL("../src/features/analyse/workbench/concordance/distribution.ts", import.meta.url), "utf8");
const ownershipCheck = readFileSync(new URL("../scripts/check-style-ownership.mjs", import.meta.url), "utf8");

for (const token of [
  "--cw-type-body-size",
  "--cw-type-help-size",
  "--cw-type-table-header-size",
  "--cw-type-table-body-size",
  "--cw-type-section-label-size",
  "--cw-type-section-heading-size",
  "--cw-type-page-title-size",
  "--cw-weight-regular",
  "--cw-weight-medium",
  "--cw-weight-semibold",
  "--cw-weight-bold",
]) {
  assert.match(base, new RegExp(`${token}:`), `base typography must define ${token}`);
}

for (const declaration of [
  "background: var(--cw-result-table-surface)",
  "background: var(--cw-result-table-header-surface)",
  "background: var(--cw-result-table-row-alt-surface)",
  "background: var(--cw-result-table-hover-surface)",
  "border-bottom: 1px solid var(--cw-result-table-separator)",
]) {
  assert(tables.includes(declaration), `tables.css must own ${declaration}`);
}
assert.match(tables, /\.cw-result-table th \{[\s\S]*font-family: var\(--cw-font-ui\);[\s\S]*font-weight: var\(--cw-weight-medium\);/);
assert.match(tables, /\.cw-result-table-numeric \{[\s\S]*font-variant-numeric: tabular-nums;[\s\S]*font-weight: var\(--cw-weight-regular\);/);
assert.match(tables, /\.cw-result-table-kwic \{[\s\S]*font-family: var\(--cw-font-mono\);/);

assert.doesNotMatch(frequencyCss, /background:\s*var\(--cw-result-table-(?:surface|header-surface|row-alt-surface|hover-surface)\)/);
assert.doesNotMatch(concordanceCss, /background:\s*var\(--cw-result-table-(?:surface|header-surface|row-alt-surface)\)/);
assert.match(html, /class="frequency-table cw-result-table"/);
assert.match(html, /class="concordance-collocations-table cw-result-table cw-result-table--compact"/);
assert.match(distribution, /concordance-distribution-table cw-result-table cw-result-table--compact/);

assert.match(panels, /\.cw-panel \{[\s\S]*border: 1px solid var\(--border-color\);[\s\S]*border-radius: 6px;[\s\S]*background: var\(--panel-bg\);[\s\S]*box-shadow:/);
assert.match(panels, /\.cw-panel__header \{/);
assert.match(panels, /\.cw-panel__controls \{/);
assert.match(panels, /\.cw-panel__body \{/);
assert.doesNotMatch(concordanceCss.match(/\.concordance-card \{([^}]*)\}/)?.[1] ?? "", /background|border(?:-radius)?|box-shadow/);

assert.match(controls, /\.primary-btn,/);
assert.match(controls, /\.cw-toggle \{/);
assert.match(controls, /\.settings-input,/);
assert.doesNotMatch(featureCss, /font-weight:\s*(?:400|500|600|700)\b/);
assert.doesNotMatch(featureCss, /font-size:\s*[^;{}]*\b\d*\.?\d+(?:px|rem|em|pt|%)/);
assert.doesNotMatch(featureCss, /\bfont\s*:\s*[^;{}]*\b\d*\.?\d+(?:px|rem|em|pt|%)/);
assert.match(ownershipCheck, /featurePanelAllowList/);
assert.match(ownershipCheck, /feature selector reimplements the shared panel contract/);
assert.match(ownershipCheck, /feature control must use shared control typography/);
assert.match(ownershipCheck, /feature control must use the shared focus outline/);
assert.match(ownershipCheck, /static export typography must remain in plots\/figures\.css/);
for (const entry of [".file-card", ".full-ocr-page-card"]) {
  assert.match(ownershipCheck, new RegExp(`selector: "${entry.replace(".", "\\.")}"[\\s\\S]*?property:[\\s\\S]*?reason:`));
}

for (const match of featureCss.matchAll(/([^{}]*(?:ready|building|build-complete|result-state)[^{}]*)\{([^{}]*)\}/gi)) {
  assert.doesNotMatch(match[2], /font-(?:family|weight|size|stretch)|letter-spacing/, `state wrapper must preserve inherited typography: ${match[1].trim()}`);
}

assert.match(concordanceCss, /\.concordance-context-token\.is-sort-level-1[\s\S]*var\(--cw-kwic-sort-1\)/);
assert.match(concordanceCss, /\.concordance-node \{[\s\S]*color: var\(--accent-color\)/);
