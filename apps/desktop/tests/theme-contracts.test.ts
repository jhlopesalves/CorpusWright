import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  appearanceThemeIds,
  findTopLevelCssBlocks,
  parseCompleteThemes,
} from "../scripts/css-contracts.mjs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const desktopRoot = path.resolve("apps/desktop");
const graph = await loadStyleGraph(new URL("../src/styles.css", import.meta.url));
const themesPath = path.join(desktopRoot, "src/styles/foundations/themes.css");
const overridesPath = path.join(desktopRoot, "src/styles/foundations/theme-overrides.css");
const themesModule = graph.modules.find(module => module.path === themesPath);
const overridesModule = graph.modules.find(module => module.path === overridesPath);
assert(themesModule, "style graph must contain the complete theme contract module");
assert(overridesModule, "style graph must contain the explicit theme-exception module");

const themes = parseCompleteThemes(themesModule.contents, "foundations/themes.css");
assert.equal(themes.length, 14, "exactly fourteen complete theme variants must exist");
assert.equal(new Set(themes.map(theme => theme.id)).size, 14, "complete theme ids must be unique");

const appearanceSource = readFileSync(new URL("../src/appearance.ts", import.meta.url), "utf8");
assert.deepEqual(
  themes.map(theme => theme.id).sort(),
  appearanceThemeIds(appearanceSource).sort(),
  "CSS theme variants must agree with the appearance theme families",
);

const contract = [...themes[0].declarations.keys()].sort();
for (const theme of themes) {
  assert.deepEqual([...theme.declarations.keys()].sort(), contract, `${theme.id} must expose the complete token contract`);
  for (const token of [
    "--cw-result-table-surface",
    "--cw-result-table-header-surface",
    "--cw-result-table-row-alt-surface",
    "--cw-result-table-border",
    "--cw-plot-point",
    "--cw-plot-fit",
    "--cw-plot-grid",
    "--cw-plot-ramp-0",
    "--cw-plot-ramp-5",
    "--cw-position-profile-bar",
    "--cw-position-profile-bar-active",
    "--cw-native-titlebar-color",
  ]) {
    assert(theme.declarations.has(token), `${theme.id} must define ${token}`);
  }
}

for (const block of findTopLevelCssBlocks(overridesModule.contents)) {
  assert.match(block.selector, /\[data-theme=/, "theme overrides must be explicit theme selectors");
  assert.doesNotMatch(block.selector, /^\[data-theme=["'][^"']+["']\]$/, "theme overrides must not look like complete contracts");
}

assert.doesNotMatch(appearanceSource, /#[0-9a-f]{3,8}\b/i, "appearance.ts must not duplicate the title-bar palette");
assert.match(appearanceSource, /getComputedStyle\(root\).*--cw-native-titlebar-color/s, "native title-bar colour must be read from the computed theme token");
assert.match(appearanceSource, /invoke\("set_window_titlebar_color", \{ color: titleBarColor \}\)/, "native title-bar invocation must retain its contract");

const rawColour = /(?<!&)#(?:[0-9a-fA-F]{3,8})\b|rgba?\([^)\n]+\)|hsla?\([^)\n]+\)/;
for (const module of graph.modules) {
  if (module.path === themesPath) continue;
  assert.doesNotMatch(module.contents, rawColour, `raw component colours are prohibited in ${path.relative(desktopRoot, module.path)}`);
}

const contrastOutput = execFileSync("node", ["scripts/check-theme-contrast.mjs"], {
  cwd: desktopRoot,
  encoding: "utf8",
});
assert.match(contrastOutput, /exactly 14 complete themes/, "contrast validation must report only complete themes");
