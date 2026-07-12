import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseCompleteThemes, resolveToken } from "./css-contracts.mjs";
import { loadStyleGraph } from "./style-graph.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const themesPath = path.join(desktopRoot, "src/styles/foundations/themes.css");
const checks = [
  { label: "main text on app background", fg: "--text-color", bg: "--bg-color", min: 4.5 },
  { label: "main text on panel background", fg: "--text-color", bg: "--panel-bg", min: 4.5 },
  { label: "muted text on app background", fg: "--text-muted", bg: "--bg-color", min: 3 },
  { label: "muted text on panel background", fg: "--text-muted", bg: "--panel-bg", min: 3 },
  { label: "primary button text", fg: "--cw-primary-text", bg: "--cw-primary-bg", min: 4.5 },
  { label: "danger button text", fg: "--cw-danger-button-text", bg: "--cw-danger-bg", min: 4.5 },
  { label: "current search match", fg: "--cw-search-current-text", bg: "--cw-search-current-bg", min: 4.5 },
  { label: "danger text on app background", fg: "--cw-danger-text", bg: "--bg-color", min: 4.5 },
  { label: "warning text on app background", fg: "--cw-warning-text", bg: "--bg-color", min: 4.5 },
  { label: "info text on app background", fg: "--cw-info-text", bg: "--bg-color", min: 4.5 },
  { label: "success text on app background", fg: "--cw-success-text", bg: "--bg-color", min: 4.5 },
  { label: "mixed category text on app background", fg: "--cw-category-mixed-text", bg: "--bg-color", min: 4.5 },
  { label: "numeric category text on app background", fg: "--cw-category-numeric-text", bg: "--bg-color", min: 4.5 },
];

function expandHex(hex) {
  return hex.length === 3 || hex.length === 4 ? [...hex].map(char => char + char).join("") : hex;
}

function parseColour(value) {
  const hexMatch = value.trim().match(/^#([0-9a-fA-F]{3,8})$/);
  if (hexMatch) {
    const hex = expandHex(hexMatch[1]);
    return {
      r: Number.parseInt(hex.slice(0, 2), 16),
      g: Number.parseInt(hex.slice(2, 4), 16),
      b: Number.parseInt(hex.slice(4, 6), 16),
      a: hex.length === 8 ? Number.parseInt(hex.slice(6, 8), 16) / 255 : 1,
    };
  }
  const rgbMatch = value.trim().match(/^rgba?\(([^)]+)\)$/);
  if (!rgbMatch) return null;
  const parts = rgbMatch[1].split(",").map(part => Number.parseFloat(part.trim()));
  if (parts.length < 3) return null;
  return { r: parts[0], g: parts[1], b: parts[2], a: parts[3] ?? 1 };
}

function composite(foreground, background) {
  return {
    r: foreground.r * foreground.a + background.r * (1 - foreground.a),
    g: foreground.g * foreground.a + background.g * (1 - foreground.a),
    b: foreground.b * foreground.a + background.b * (1 - foreground.a),
    a: 1,
  };
}

function colourFor(token, declarations, baseBackground) {
  const resolved = resolveToken(token, declarations);
  if (!resolved) return null;
  const colour = parseColour(resolved);
  if (!colour) return null;
  return colour.a < 1 && baseBackground ? composite(colour, baseBackground) : colour;
}

function luminance(colour) {
  const channel = value => {
    const scaled = value / 255;
    return scaled <= 0.03928 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b);
}

function contrast(first, second) {
  const values = [luminance(first), luminance(second)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

const graph = await loadStyleGraph();
const themesModule = graph.modules.find(module => module.path === themesPath);
if (!themesModule) throw new Error("The style graph does not import foundations/themes.css");
const themes = parseCompleteThemes(themesModule.contents, "src/styles/foundations/themes.css");
const failures = [];
if (themes.length !== 16) failures.push(`expected exactly 16 complete themes, found ${themes.length}`);

for (const theme of themes) {
  const baseBackground = colourFor("--bg-color", theme.declarations);
  for (const check of checks) {
    const bg = colourFor(check.bg, theme.declarations, baseBackground);
    const fg = colourFor(check.fg, theme.declarations, bg ?? baseBackground);
    if (!fg || !bg) {
      failures.push(`${theme.id}: ${check.label} could not parse ${check.fg} / ${check.bg}`);
      continue;
    }
    const ratio = contrast(fg, bg);
    if (ratio < check.min) failures.push(`${theme.id}: ${check.label} is ${ratio.toFixed(2)}:1, expected at least ${check.min}:1`);
  }
}

if (failures.length) {
  console.error("Theme contrast check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Theme contrast check passed for exactly ${themes.length} complete themes (${checks.length} checks each).`);
