import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { parseCompleteThemes } from "../scripts/css-contracts.mjs";
import {
  colourForToken,
  contrastRatio,
  deltaE76,
} from "../scripts/colour-contracts.mjs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";
import {
  MINIMUM_RAMP_DELTA_E_76,
  validateVisualContracts,
} from "../scripts/visual-contracts.mjs";

const desktopRoot = path.resolve("apps/desktop");
const graph = await loadStyleGraph(new URL("../src/styles.css", import.meta.url));
const themesPath = path.join(desktopRoot, "src/styles/foundations/themes.css");
const themeSource = graph.modules.find(module => module.path === themesPath)?.contents ?? "";
const typescriptSources = ["figure.ts", "render.ts", "svg-export.ts"].map(file => ({
  path: path.join(desktopRoot, "src/plots", file),
  contents: readFileSync(path.join(desktopRoot, "src/plots", file), "utf8"),
}));

assert.deepEqual(validateVisualContracts({
  modules: graph.modules,
  desktopRoot,
  themeSource,
  typescriptSources,
}), []);

const invalidCss = `
.cw-plot-point.is-excluded { stroke-color: red; }
.fixture-focus { outline: 2px solid var(--cw-interaction-focus-ring); box-shadow: var(--cw-interaction-focus-outline); }
.fixture-type { font-size: 13px; font-weight: 650; }
.fixture-spinner { animation: fixture-spin 1s linear infinite; }
.fixture-point { opacity: 0; animation: fixture-in 1s forwards; }
`;
const invalidThemeSource = themeSource.replace("--cw-plot-ramp-5: #c084fc", "--cw-plot-ramp-5: #9076ff");
const invalid = validateVisualContracts({
  modules: [...graph.modules, { path: path.join(desktopRoot, "src/styles/features/invalid-fixture.css"), contents: invalidCss }],
  desktopRoot,
  themeSource: invalidThemeSource,
  typescriptSources: [...typescriptSources, { path: "fixture.ts", contents: "const FIGURE_TITLE_FONT_SIZE = 17; const weight = 650;" }],
  exportFixtures: [{ path: "fixture.svg", contents: '<svg style="fill:var(--cw-plot-point)"></svg>' }],
});
for (const expected of [
  "invalid stroke-color",
  "focus-shadow token used in outline",
  "focus-outline token used as box shadow",
  "raw numeric font-size",
  "numeric interface font-weight",
  "exactly one generic owner",
  "indefinite animation fixture-spin lacks",
  "leaves .fixture-point transparent",
  "export identity typography constant",
  "intermediate weight 650",
  "unresolved CSS variable",
  "exact duplicate plot-ramp steps",
]) {
  assert(invalid.some(violation => violation.includes(expected)), `fixture must trigger ${expected}`);
}

const themes = parseCompleteThemes(themeSource);
assert.equal(themes.length, 16);
for (const theme of themes) {
  const panel = colourForToken("--panel-bg", theme.declarations);
  const ramps = Array.from({ length: 6 }, (_, index) => colourForToken(`--cw-plot-ramp-${index}`, theme.declarations));
  assert.equal(new Set(ramps.map(colour => `${colour.r},${colour.g},${colour.b},${colour.a}`)).size, 6, `${theme.id} ramps must be distinct`);
  for (const [index, ramp] of ramps.entries()) {
    assert(contrastRatio(ramp, panel) >= 3, `${theme.id} ramp ${index} must contrast with the plot surface`);
    if (index > 0) {
      assert(
        deltaE76(ramps[index - 1]!, ramp) >= MINIMUM_RAMP_DELTA_E_76,
        `${theme.id} ramps ${index - 1}/${index} must remain perceptually distinct`,
      );
    }
  }
}

const contrastOutput = execFileSync("node", ["scripts/check-theme-contrast.mjs"], {
  cwd: desktopRoot,
  encoding: "utf8",
});
assert.match(contrastOutput, /exactly 16 complete themes \(42 checks each\)/);
const visualOutput = execFileSync("node", ["scripts/check-visual-contracts.mjs"], {
  cwd: desktopRoot,
  encoding: "utf8",
});
assert.match(visualOutput, /Visual contract check passed/);
