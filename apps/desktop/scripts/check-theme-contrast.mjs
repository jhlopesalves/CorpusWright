import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseCompleteThemes } from "./css-contracts.mjs";
import {
  colourForToken,
  compositeColours,
  contrastRatio,
} from "./colour-contracts.mjs";
import { loadStyleGraph } from "./style-graph.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const themesPath = path.join(desktopRoot, "src/styles/foundations/themes.css");

export const contrastChecks = [
  { label: "main text on app background", fg: "--text-color", bg: "--bg-color", min: 4.5 },
  { label: "main text on panel", fg: "--text-color", bg: "--panel-bg", min: 4.5 },
  { label: "small muted text on app background", fg: "--text-muted", bg: "--bg-color", min: 4.5 },
  { label: "small muted text on panel", fg: "--text-muted", bg: "--panel-bg", min: 4.5 },
  { label: "result-table header text", fg: "--cw-result-table-header-text", bg: "--cw-result-table-header-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "result-table body text on base row", fg: "--cw-result-table-body-text", bg: "--cw-result-table-row-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "result-table body text on alternate row", fg: "--cw-result-table-body-text", bg: "--cw-result-table-row-alt-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "result-table muted text on base row", fg: "--cw-result-table-muted-text", bg: "--cw-result-table-row-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "result-table muted text on alternate row", fg: "--cw-result-table-muted-text", bg: "--cw-result-table-row-alt-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "selected table text", fg: "--cw-text-selected", bg: "--cw-result-table-active-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "tooltip text", fg: "--text-color", bg: "--cw-surface-elevated", min: 4.5 },
  { label: "tooltip muted text", fg: "--text-muted", bg: "--cw-surface-elevated", min: 4.5 },
  { label: "KWIC node on base row", fg: "--accent-color", bg: "--cw-result-table-row-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  { label: "KWIC node on alternate row", fg: "--accent-color", bg: "--cw-result-table-row-alt-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  ...[1, 2, 3].flatMap(level => [
    { label: `KWIC Sort ${level} on base row`, fg: `--cw-kwic-sort-${level}`, bg: "--cw-result-table-row-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
    { label: `KWIC Sort ${level} on alternate row`, fg: `--cw-kwic-sort-${level}`, bg: "--cw-result-table-row-alt-surface", bgBase: "--cw-result-table-surface", min: 4.5 },
  ]),
  { label: "primary button text", fg: "--cw-primary-text", bg: "--cw-primary-bg", min: 4.5 },
  { label: "danger button text", fg: "--cw-danger-button-text", bg: "--cw-danger-bg", min: 4.5 },
  { label: "danger text", fg: "--cw-danger-text", bg: "--bg-color", min: 4.5 },
  { label: "warning text", fg: "--cw-warning-text", bg: "--bg-color", min: 4.5 },
  { label: "informational text", fg: "--cw-info-text", bg: "--bg-color", min: 4.5 },
  { label: "success text", fg: "--cw-success-text", bg: "--bg-color", min: 4.5 },
  { label: "mixed category text", fg: "--cw-category-mixed-text", bg: "--bg-color", min: 4.5 },
  { label: "numeric category text", fg: "--cw-category-numeric-text", bg: "--bg-color", min: 4.5 },
  { label: "current search match", fg: "--cw-search-current-text", bg: "--cw-search-current-bg", min: 4.5 },
  { label: "focus indicator on app background", fg: "--focus-ring", bg: "--bg-color", min: 3 },
  { label: "focus indicator on panel", fg: "--focus-ring", bg: "--panel-bg", min: 3 },
  { label: "result-table bar against track", fg: "--cw-result-table-bar", bg: "--cw-result-table-track", bgBase: "--cw-result-table-surface", min: 3 },
  { label: "position-profile bar on base row", fg: "--cw-position-profile-bar", bg: "--cw-result-table-row-surface", bgBase: "--cw-result-table-surface", min: 3 },
  { label: "position-profile bar on alternate row", fg: "--cw-position-profile-bar", bg: "--cw-result-table-row-alt-surface", bgBase: "--cw-result-table-surface", min: 3 },
  { label: "plot point on panel", fg: "--cw-plot-point", bg: "--panel-bg", min: 3 },
  { label: "model-fit line on panel", fg: "--cw-theme-plot-model-fit", bg: "--panel-bg", min: 3 },
  { label: "statistical reference on panel", fg: "--cw-theme-plot-statistical-reference", bg: "--panel-bg", min: 3 },
  { label: "selected target on panel", fg: "--cw-theme-plot-target", bg: "--panel-bg", min: 3 },
  { label: "coverage trajectory on panel", fg: "--cw-theme-plot-coverage-trajectory", bg: "--panel-bg", min: 3 },
  ...Array.from({ length: 6 }, (_, level) => ({
    label: `plot ramp ${level} on panel`,
    fg: `--cw-plot-ramp-${level}`,
    bg: "--panel-bg",
    min: 3,
  })),
];

const paletteRoleTokens = {
  theme: [
    "--cw-plot-point", "--cw-theme-plot-model-fit", "--cw-theme-plot-statistical-reference", "--cw-theme-plot-target",
    "--cw-plot-ramp-4", "--cw-plot-ramp-5",
    "--cw-plot-point",
    "--cw-plot-ramp-0", "--cw-plot-ramp-5", "--cw-plot-ramp-0", "--cw-plot-ramp-5",
    "--cw-theme-plot-coverage-trajectory",
  ],
  colorBlind: [
    "--accent-color", "--cw-theme-plot-model-fit", "--cw-theme-plot-statistical-reference", "--cw-theme-plot-target",
    "--cw-category-mixed-text", "--cw-danger-text",
    "--accent-color",
    "--cw-plot-ramp-0", "--cw-plot-ramp-5", "--cw-plot-ramp-0", "--cw-plot-ramp-5",
    "--cw-theme-plot-coverage-trajectory",
  ],
  monochrome: [
    "--text-color", "--text-color", "--text-muted", "--text-color",
    "--text-color", "--text-color",
    "--text-color",
    "--text-muted", "--text-color", "--text-muted", "--text-color",
    "--text-color",
  ],
};

const paletteRoleLabels = [
  "observed", "model fit", "statistical reference", "selected target", "selected", "attention",
  "witness",
  "density low", "density high", "composition low", "composition high",
  "coverage trajectory",
];

export function validatePaletteContrast(themes) {
  const failures = [];
  for (const theme of themes) {
    const background = opaqueTokenColour("--panel-bg", theme.declarations);
    for (const [mode, tokens] of Object.entries(paletteRoleTokens)) {
      tokens.forEach((token, index) => {
        try {
          const foreground = opaqueTokenColour(token, theme.declarations, "--panel-bg");
          const ratio = contrastRatio(foreground, background);
          if (ratio < 3) {
            failures.push(`${theme.id} × ${mode}: ${paletteRoleLabels[index]} is ${ratio.toFixed(2)}:1, expected at least 3:1`);
          }
        } catch (error) {
          failures.push(`${theme.id} × ${mode}: ${paletteRoleLabels[index]} could not parse ${token}: ${error instanceof Error ? error.message : String(error)}`);
        }
      });
    }
  }
  return failures;
}

export function validateThemeContrast(themes, checks = contrastChecks) {
  const failures = [];
  for (const theme of themes) {
    for (const check of checks) {
      try {
        const background = opaqueTokenColour(check.bg, theme.declarations, check.bgBase);
        const foreground = opaqueTokenColour(check.fg, theme.declarations, check.bg);
        const ratio = contrastRatio(foreground, background);
        if (ratio < check.min) {
          failures.push(`${theme.id}: ${check.label} is ${ratio.toFixed(2)}:1, expected at least ${check.min}:1`);
        }
      } catch (error) {
        failures.push(`${theme.id}: ${check.label} could not parse ${check.fg} / ${check.bg}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }
  return failures;
}

function opaqueTokenColour(token, declarations, backdropToken) {
  const colour = colourForToken(token, declarations);
  if (colour.a >= 1) return colour;
  if (!backdropToken || backdropToken === token) throw new Error(`${token} is translucent without a backdrop contract`);
  const backdrop = opaqueTokenColour(backdropToken, declarations, "--bg-color");
  return compositeColours(colour, backdrop);
}

const graph = await loadStyleGraph();
const themesModule = graph.modules.find(module => module.path === themesPath);
if (!themesModule) throw new Error("The style graph does not import foundations/themes.css");
const themes = parseCompleteThemes(themesModule.contents, "src/styles/foundations/themes.css");
const failures = [];
if (themes.length !== 14) failures.push(`expected exactly 14 complete themes, found ${themes.length}`);
if (contrastChecks.length !== 45) failures.push(`expected exactly 45 contrast checks, found ${contrastChecks.length}`);
failures.push(...validateThemeContrast(themes));
failures.push(...validatePaletteContrast(themes));

if (failures.length) {
  console.error("Theme contrast check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Theme contrast check passed for exactly ${themes.length} complete themes (${contrastChecks.length} base checks and ${Object.keys(paletteRoleTokens).length * paletteRoleLabels.length} theme × palette role checks each).`);
