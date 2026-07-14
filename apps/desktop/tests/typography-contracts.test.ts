import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { findTopLevelCssBlocks } from "../scripts/css-contracts.mjs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const desktopRoot = path.resolve("apps/desktop");
const graph = await loadStyleGraph(new URL("../src/styles.css", import.meta.url));
const moduleSource = (relativePath: string): string => {
  const expected = path.join(desktopRoot, "src/styles", relativePath);
  const module = graph.modules.find(candidate => candidate.path === expected);
  assert(module, `${relativePath} must be present in the style graph`);
  return module.contents;
};
const base = moduleSource("foundations/base.css");
const fonts = moduleSource("foundations/fonts.css");
const controls = moduleSource("components/controls.css");
const tables = moduleSource("components/tables.css");
const analyse = moduleSource("features/analyse.css");
const plotCss = graph.modules
  .filter(module => module.path.includes(`${path.sep}styles${path.sep}plots${path.sep}`))
  .map(module => module.contents)
  .join("\n");
const allCss = graph.modules.map(module => module.contents).join("\n");

const semanticSizeTokens = [
  "--cw-type-workspace-title-size",
  "--cw-type-page-title-size",
  "--cw-type-dialog-title-size",
  "--cw-type-icon-size",
  "--cw-type-subsection-heading-size",
  "--cw-type-section-heading-size",
  "--cw-type-section-label-size",
  "--cw-type-body-size",
  "--cw-type-body-compact-size",
  "--cw-type-ui-size",
  "--cw-type-control-size",
  "--cw-type-help-size",
  "--cw-type-compact-size",
  "--cw-type-caption-size",
  "--cw-type-micro-size",
  "--cw-type-micrographic-size",
  "--cw-type-code-size",
  "--cw-type-status-size",
  "--cw-type-table-header-size",
  "--cw-type-table-body-size",
  "--cw-type-plot-title-size",
  "--cw-type-plot-subtitle-size",
  "--cw-type-plot-label-size",
  "--cw-type-plot-tick-size",
  "--cw-type-plot-tooltip-size",
  "--cw-type-plot-micro-size",
];
for (const token of semanticSizeTokens) {
  assert.match(base, new RegExp(`${token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}:\\s*[^;]+;`));
  const consumers = [...allCss.matchAll(new RegExp(`var\\(${token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\)`, "g"))];
  assert(consumers.length > 0, `${token} must have a real consumer`);
}

const rawSize = /\bfont-size\s*:\s*[^;{}]*\b\d*\.?\d+(?:px|rem|em|pt|%|vw|vh)\b/;
const rawShorthandSize = /\bfont\s*:\s*[^;{}]*\b\d*\.?\d+(?:px|rem|em|pt|%|vw|vh)\b/;
for (const module of graph.modules) {
  if (module.path.endsWith(`${path.sep}foundations${path.sep}base.css`)) continue;
  assert.doesNotMatch(module.contents, rawSize, `raw font sizes are prohibited in ${module.path}`);
  assert.doesNotMatch(module.contents, rawShorthandSize, `raw shorthand font sizes are prohibited in ${module.path}`);
}

const interfaceCss = graph.modules
  .filter(module => !module.path.endsWith(`${path.sep}foundations${path.sep}fonts.css`))
  .map(module => module.contents)
  .join("\n");
assert.doesNotMatch(interfaceCss, /\bfont-weight\s*:\s*[1-9]\d{2}\s*;/, "interface weights must use semantic tokens");
assert.doesNotMatch(`${allCss}\n${readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8")}`, /\b650\b/);
assert.match(base, /html\s*\{[^}]*font-synthesis:\s*none/s, "synthetic weights must be disabled at the inherited root");

assert.match(analyse, /\.analyse-header h1\s*\{[^}]*font-size:\s*var\(--cw-type-page-title-size\)/s);
assert.match(analyse, /\.analyse-eyebrow\s*\{[^}]*font-size:\s*var\(--cw-type-section-label-size\)/s);
assert.match(analyse, /\.analyse-section h2\s*\{[^}]*font-size:\s*var\(--cw-type-section-heading-size\)/s);
assert.match(analyse, /\.analyse-intro[\s\S]*font-size:\s*var\(--cw-type-help-size\)/);
assert.match(controls, /\.primary-btn,[\s\S]*font-size:\s*var\(--cw-type-control-size\)/);
for (const token of [
  "--cw-type-plot-title-size",
  "--cw-type-plot-subtitle-size",
  "--cw-type-plot-label-size",
  "--cw-type-plot-tick-size",
  "--cw-type-plot-tooltip-size",
]) {
  assert(plotCss.includes(`var(${token})`), `${token} must govern plot text`);
}

assert.match(tables, /\.cw-result-table\s*\{[^}]*font-family:\s*var\(--cw-font-ui\)/s);
assert.match(tables, /\.cw-result-table td\.cw-result-table-kwic,[^}]*font-family:\s*var\(--cw-font-mono\)/s);
assert.doesNotMatch(tables, /\.cw-result-table(?:-numeric|-lexical)?\s*\{[^}]*font-family:\s*var\(--cw-font-mono\)/s);

for (const block of findTopLevelCssBlocks(allCss)) {
  if (/(?:analysis|analyse).*(?:ready|building|build-complete|result-state)/i.test(block.selector)) {
    assert.doesNotMatch(
      block.body,
      /font-(?:family|weight|size|stretch|synthesis)|letter-spacing/,
      `analysis state wrapper cannot alter typography: ${block.selector}`,
    );
  }
}
for (const [selector, token] of [
  [".analyse-eyebrow", "--cw-type-section-label-size"],
  [".analyse-header h1", "--cw-type-page-title-size"],
  [".analyse-tool-rail h2", "--cw-type-section-label-size"],
  [".analyse-section h2", "--cw-type-section-heading-size"],
] as const) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  assert.match(analyse, new RegExp(`${escaped}\\s*\\{[^}]*font-size:\\s*var\\(${token}\\)`, "s"));
}

const appearance = readFileSync(new URL("../src/app/appearance.ts", import.meta.url), "utf8");
const optionNames = (constant: "FONT_OPTIONS" | "MONO_FONT_OPTIONS"): string[] => {
  const block = appearance.match(new RegExp(`export const ${constant}[^=]*=\\s*\\[([\\s\\S]*?)\\n\\];`))?.[1] ?? "";
  return [...block.matchAll(/\bname:\s*"([^"]+)"/g)].map(match => match[1]);
};
const faceWeights = new Map<string, Array<[number, number]>>();
for (const block of fonts.matchAll(/@font-face\s*\{([^}]+)\}/g)) {
  const family = block[1]!.match(/font-family:\s*"([^"]+)"/)?.[1];
  const weight = block[1]!.match(/font-weight:\s*(\d+)(?:\s+(\d+))?/) ?? [];
  if (!family || !weight[1]) continue;
  const lower = Number(weight[1]);
  const upper = Number(weight[2] ?? weight[1]);
  faceWeights.set(family, [...(faceWeights.get(family) ?? []), [lower, upper]]);
}
const selectableFamilies = [...optionNames("FONT_OPTIONS"), ...optionNames("MONO_FONT_OPTIONS")];
assert.equal(new Set(selectableFamilies).size, selectableFamilies.length, "selectable font names must be unique");
for (const family of selectableFamilies) {
  if (family === "System UI") continue;
  const ranges = faceWeights.get(family);
  assert(ranges && ranges.length > 0, `${family} must have a declared bundled face`);
  for (const requested of [400, 500, 600, 700]) {
    const supplied = ranges.some(([lower, upper]) => requested >= lower && requested <= upper);
    assert(supplied || /font-synthesis:\s*none/.test(base), `${family} ${requested} must resolve without synthesis`);
  }
}
