import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { parseCompleteThemes } from "../scripts/css-contracts.mjs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const desktopRoot = path.resolve("apps/desktop");
const entryPath = path.join(desktopRoot, "src/styles.css");
const expectedModules = [
  "foundations/fonts.css",
  "foundations/themes.css",
  "foundations/base.css",
  "layout/app-shell.css",
  "layout/workbenches.css",
  "components/navigation.css",
  "components/controls.css",
  "components/panels.css",
  "components/tables.css",
  "components/modals.css",
  "components/feedback.css",
  "plots/figures.css",
  "plots/axes-and-grids.css",
  "plots/marks.css",
  "plots/legends-and-annotations.css",
  "plots/interactions.css",
  "features/project.css",
  "features/prepare.css",
  "features/analyse.css",
  "features/frequency.css",
  "features/concordance.css",
  "features/sources-and-preview.css",
  "features/repeated-artifacts.css",
  "features/settings.css",
  "foundations/theme-overrides.css",
  "layout/responsive.css",
];

const entry = readFileSync(entryPath, "utf8");
const imports = [...entry.matchAll(/@import "\.\/styles\/([^"]+)";/g)].map(match => match[1]);
assert.deepEqual(imports, expectedModules, "styles.css must document the exact cascade through ordered imports");
assert.equal(
  entry.replace(/\/\*[\s\S]*?\*\//g, "").replace(/@import[^;]+;/g, "").trim(),
  "",
  "styles.css must remain entrypoint-only",
);

const graph = await loadStyleGraph(entryPath);
assert.equal(graph.modules.length, expectedModules.length + 1, "all expected modules must resolve without cycles");
for (const relativePath of expectedModules) {
  const absolutePath = path.join(desktopRoot, "src/styles", relativePath);
  assert(graph.modules.some(module => module.path === absolutePath), `${relativePath} must exist in the loaded graph`);
}

const moduleSource = (relativePath: string): string => graph.modules.find(
  module => module.path === path.join(desktopRoot, "src/styles", relativePath),
)?.contents ?? "";
assert.equal(parseCompleteThemes(moduleSource("foundations/themes.css")).length, 16);
assert.match(moduleSource("components/tables.css"), /\.cw-result-table \{/);
assert.match(moduleSource("components/panels.css"), /\.cw-panel \{/);
assert.match(moduleSource("components/controls.css"), /\.primary-btn/);
assert.match(moduleSource("plots/axes-and-grids.css"), /\.cw-plot-axis-baseline/);
assert.match(moduleSource("plots/marks.css"), /\.cw-plot-point/);
assert.match(moduleSource("plots/legends-and-annotations.css"), /\.cw-plot-legend-bg/);
assert.match(moduleSource("plots/interactions.css"), /\.cw-plot-tooltip/);

const featureCss = expectedModules
  .filter(file => file.startsWith("features/"))
  .map(moduleSource)
  .join("\n");
assert.doesNotMatch(featureCss, /\.cw-result-table\s*\{/);
assert.doesNotMatch(featureCss, /\.cw-plot-(?:axis|grid|legend|tooltip)[\w-]*\s*\{/);
assert.doesNotMatch(expectedModules.join("\n"), /migration-compatibility|legacy\.css|temporary\.css/);
assert.doesNotMatch(readFileSync(path.join(desktopRoot, "index.html"), "utf8"), /\sstyle\s*=/i);
