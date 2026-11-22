import path from "node:path";
import { fileURLToPath } from "node:url";
import { resolveCompleteThemes } from "./css-contracts.mjs";
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
  ...Array.from({ length: 5 }, (_, level) => ({
    label: `plot ramp ${level} on panel`,
    fg: `--cw-plot-ramp-${level}`,
    bg: "--panel-bg",
    min: 3,
  })),
];

const navigationChecks = ["bg", "hover", "selected", "selected-hover"].flatMap(ground => [
  { label: `navigation text on ${ground}`, fg: "--cw-navigation-text", bg: `--cw-navigation-${ground}`, min: 4.5 },
  { label: `navigation secondary text on ${ground}`, fg: "--cw-navigation-muted", bg: `--cw-navigation-${ground}`, min: 4.5 },
  { label: `navigation focus on ${ground}`, fg: "--cw-navigation-focus", bg: `--cw-navigation-${ground}`, min: 3 },
]);

const componentChecks = [
  { label: "node ink", fg: "--cw-node-ink", bg: "--cw-node-bg", min: 4.5 },
  { label: "node underline", fg: "--cw-node-line", bg: "--cw-node-bg", min: 3 },
  { label: "selection ink", fg: "--cw-selected-ink", bg: "--cw-selected-bg", min: 4.5 },
  { label: "Original Text label", fg: "--cw-original-ink", bg: "--cw-original-bg", min: 4.5 },
  { label: "Processed Text label", fg: "--cw-processed-ink", bg: "--cw-processed-bg", min: 4.5 },
  { label: "collocation ink", fg: "--cw-collocate-text", bg: "--cw-collocate-bg", bgBase: "--panel-bg", min: 4.5 },
  { label: "muted status ink", fg: "--cw-neutral-text", bg: "--cw-neutral-bg", bgBase: "--panel-bg", min: 4.5 },
  { label: "warning ink", fg: "--cw-warning-text", bg: "--cw-warning-bg", bgBase: "--panel-bg", min: 4.5 },
  { label: "danger ink in a status", fg: "--cw-danger-text", bg: "--cw-danger-bg-subtle", bgBase: "--panel-bg", min: 4.5 },
  { label: "accent ink on hover", fg: "--cw-accent-ink", bg: "--cw-interaction-bg-hover", min: 4.5 },
  { label: "project path", fg: "--cw-text-metadata", bg: "--cw-header-bg", min: 4.5 },
  { label: "field boundary on app", fg: "--cw-field-border", bg: "--bg-color", min: 3 },
  { label: "field boundary on panel", fg: "--cw-field-border", bg: "--panel-bg", min: 3 },
];

const wrightChecks = [
  ...["active", "selected", "range", "source", "target", "success", "warning", "muted"].flatMap(role => [
    { label: `${role} ink`, fg: `--visual-${role}-ink`, bg: `--visual-${role}-bg`, min: 4.5 },
    { label: `${role} line`, fg: `--visual-${role}-line`, bg: `--visual-${role}-bg`, min: 3 },
  ]),
  ...["app", "surface", "surface-subtle", "header", "hover", "selected"].flatMap(ground =>
    ["primary", "secondary", "tertiary"].map(role => ({
      label: `${role} text on ${ground}`, fg: `--color-text-${role}`, bg: `--color-${ground}`, min: 4.5,
    })),
  ),
  ...["hover", "selected", "surface", "surface-subtle", "accent-subtle"].map(ground => ({
    label: `accent ink on ${ground}`, fg: "--color-accent-ink", bg: `--color-${ground}`, min: 4.5,
  })),
  ...["surface", "surface-subtle"].flatMap(ground =>
    Array.from({ length: 5 }, (_, level) => ({
      label: `Wright chart step ${level} on ${ground}`, fg: `--chart-seq-${level}`, bg: `--color-${ground}`, min: 3,
    })),
  ),
  { label: "primary text during hover", fg: "--color-on-accent", bg: "--color-accent-hover", min: 4.5 },
];

const paletteRoleTokens = {
  theme: [
    "--cw-plot-point", "--cw-theme-plot-model-fit", "--cw-theme-plot-statistical-reference", "--cw-theme-plot-target",
    "--cw-theme-plot-target", "--cw-danger-text",
    "--cw-plot-point",
    "--cw-plot-ramp-0", "--cw-plot-ramp-4", "--cw-plot-ramp-0", "--cw-plot-ramp-4",
    "--cw-theme-plot-coverage-trajectory",
  ],
  colorBlind: [
    "--cw-plot-point", "--cw-theme-plot-model-fit", "--cw-theme-plot-statistical-reference", "--cw-theme-plot-target",
    "--cw-theme-plot-target", "--cw-danger-text",
    "--cw-plot-point",
    "--cw-plot-ramp-0", "--cw-plot-ramp-4", "--cw-plot-ramp-0", "--cw-plot-ramp-4",
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

const paletteChecks = Object.entries(paletteRoleTokens).flatMap(([mode, tokens]) =>
  tokens.map((fg, index) => ({
    label: `${mode}: ${paletteRoleLabels[index]}`, fg, bg: "--panel-bg", min: 3,
  })),
);

function measureContrast(declarations, { fg, bg, bgBase }) {
  const background = opaqueTokenColour(bg, declarations, bgBase);
  const foreground = compositeColours(colourForToken(fg, declarations), background);
  return contrastRatio(foreground, background);
}

export function validateThemeContrast(themes, checks = contrastChecks) {
  const failures = [];
  for (const theme of themes) {
    for (const check of checks) {
      try {
        const ratio = measureContrast(theme.declarations, check);
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
const wrightModule = graph.modules.find(module => module.path.endsWith("/foundations/wright-tokens.css"));
if (!wrightModule) throw new Error("The style graph does not import the Wright token source");
const themes = resolveCompleteThemes(themesModule.contents, wrightModule.contents);
const failures = [];
if (themes.length !== 4) failures.push(`expected exactly 4 complete themes, found ${themes.length}`);
if (contrastChecks.length !== 44) failures.push(`expected exactly 44 contrast checks, found ${contrastChecks.length}`);
failures.push(...validateThemeContrast(themes));
failures.push(...validateThemeContrast(themes, navigationChecks));
failures.push(...validateThemeContrast(themes, componentChecks));
failures.push(...validateThemeContrast(themes, paletteChecks));
failures.push(...validateThemeContrast(themes.filter(theme => theme.id.startsWith("wright-")), wrightChecks));

if (failures.length) {
  console.error("Theme contrast check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Theme contrast check passed for exactly ${themes.length} complete themes (${contrastChecks.length} base checks and ${Object.keys(paletteRoleTokens).length * paletteRoleLabels.length} theme × palette role checks each).`);
console.log(`Each theme passed ${navigationChecks.length} navigation and ${componentChecks.length} component pairings.`);
console.log(`Wright passed ${wrightChecks.length} additional pairings per variant.`);

if (process.argv.includes("--report")) {
  const names = themes.map(theme => theme.id);
  console.log(`\n| Pairing | Ink / ground | Minimum | ${names.join(" | ")} |`);
  console.log(`|---|---|---:|${names.map(() => "---:").join("|")}|`);
  const sharedChecks = [...contrastChecks, ...navigationChecks, ...componentChecks, ...paletteChecks];
  for (const check of [...sharedChecks, ...wrightChecks]) {
    const ratios = themes.map(theme => {
      if (wrightChecks.includes(check) && !theme.id.startsWith("wright-")) return "—";
      return `${measureContrast(theme.declarations, check).toFixed(2)}:1`;
    });
    console.log(`| ${check.label} | \`${check.fg}\` / \`${check.bg}\` | ${check.min}:1 | ${ratios.join(" | ")} |`);
  }
}
