import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadStyleGraph } from "./style-graph.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const stylesPath = path.join(desktopRoot, "src", "styles.css");

const checks = [
  { label: "main text on app background", fg: "--text-color", bg: "--bg-color", min: 4.5 },
  { label: "main text on panel background", fg: "--text-color", bg: "--panel-bg", min: 4.5 },
  { label: "muted text on app background", fg: "--text-muted", bg: "--bg-color", min: 3 },
  { label: "muted text on panel background", fg: "--text-muted", bg: "--panel-bg", min: 3 },
  { label: "primary button text", fg: "--cw-text-inverse", bg: "--accent-color", min: 4.5 },
  { label: "danger button text", fg: "--cw-text-inverse", bg: "--cw-danger-bg", min: 4.5 },
  { label: "current search match", fg: "--cw-search-current-text", bg: "--cw-search-current-bg", min: 4.5 },
  { label: "danger text on app background", fg: "--cw-danger-text", bg: "--bg-color", min: 4.5 },
  { label: "warning text on app background", fg: "--cw-warning-text", bg: "--bg-color", min: 4.5 },
  { label: "info text on app background", fg: "--cw-info-text", bg: "--bg-color", min: 4.5 },
  { label: "success text on app background", fg: "--cw-success-text", bg: "--bg-color", min: 4.5 },
  { label: "mixed category text on app background", fg: "--cw-category-mixed-text", bg: "--bg-color", min: 4.5 },
  { label: "numeric category text on app background", fg: "--cw-category-numeric-text", bg: "--bg-color", min: 4.5 },
];

function findClosingBrace(source, openIndex) {
  let depth = 0;
  for (let index = openIndex; index < source.length; index += 1) {
    if (source[index] === "{") {
      depth += 1;
    } else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }
  return -1;
}

function findThemeBlocks(source) {
  const blocks = [];
  let cursor = 0;

  while (cursor < source.length) {
    const openIndex = source.indexOf("{", cursor);
    if (openIndex === -1) {
      break;
    }

    const closeIndex = findClosingBrace(source, openIndex);
    if (closeIndex === -1) {
      throw new Error("Could not parse styles.css: unmatched opening brace.");
    }

    const selectorStart = Math.max(source.lastIndexOf("}", openIndex) + 1, 0);
    const selector = source.slice(selectorStart, openIndex).trim();
    if (selector.includes(":root") || selector.includes("[data-theme")) {
      const themeMatch = selector.match(/\[data-theme=["']?([^"'\]]+)["']?\]/);
      blocks.push({
        themeId: themeMatch?.[1] ?? "root",
        body: source.slice(openIndex + 1, closeIndex),
      });
    }

    cursor = closeIndex + 1;
  }

  return blocks;
}

function parseVars(body) {
  const vars = new Map();
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    vars.set(match[1], match[2].trim());
  }
  return vars;
}

function expandHex(hex) {
  if (hex.length === 3 || hex.length === 4) {
    return [...hex].map(char => char + char).join("");
  }
  return hex;
}

function parseColour(value) {
  const trimmed = value.trim();
  const hexMatch = trimmed.match(/^#([0-9a-fA-F]{3,8})$/);
  if (hexMatch) {
    const hex = expandHex(hexMatch[1]);
    const alpha = hex.length === 8 ? Number.parseInt(hex.slice(6, 8), 16) / 255 : 1;
    return {
      r: Number.parseInt(hex.slice(0, 2), 16),
      g: Number.parseInt(hex.slice(2, 4), 16),
      b: Number.parseInt(hex.slice(4, 6), 16),
      a: alpha,
    };
  }

  const rgbMatch = trimmed.match(/^rgba?\(([^)]+)\)$/);
  if (rgbMatch) {
    const parts = rgbMatch[1].split(",").map(part => part.trim());
    if (parts.length < 3) {
      return null;
    }

    return {
      r: Number.parseFloat(parts[0]),
      g: Number.parseFloat(parts[1]),
      b: Number.parseFloat(parts[2]),
      a: parts[3] === undefined ? 1 : Number.parseFloat(parts[3]),
    };
  }

  return null;
}

function resolveValue(value, vars, seen = new Set()) {
  const varMatch = value.match(/^var\((--[\w-]+)\)$/);
  if (!varMatch) {
    return value;
  }

  const token = varMatch[1];
  if (seen.has(token) || !vars.has(token)) {
    return value;
  }

  seen.add(token);
  return resolveValue(vars.get(token), vars, seen);
}

function composite(foreground, background) {
  const alpha = Math.max(0, Math.min(1, foreground.a));
  return {
    r: foreground.r * alpha + background.r * (1 - alpha),
    g: foreground.g * alpha + background.g * (1 - alpha),
    b: foreground.b * alpha + background.b * (1 - alpha),
    a: 1,
  };
}

function colourFor(token, vars, baseBackground) {
  const value = vars.get(token);
  if (!value) {
    return null;
  }

  const colour = parseColour(resolveValue(value, vars));
  if (!colour) {
    return null;
  }

  if (colour.a < 1 && baseBackground) {
    return composite(colour, baseBackground);
  }

  return colour;
}

function channelLuminance(value) {
  const scaled = value / 255;
  return scaled <= 0.03928
    ? scaled / 12.92
    : ((scaled + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(colour) {
  return 0.2126 * channelLuminance(colour.r)
    + 0.7152 * channelLuminance(colour.g)
    + 0.0722 * channelLuminance(colour.b);
}

function contrastRatio(first, second) {
  const firstLum = relativeLuminance(first);
  const secondLum = relativeLuminance(second);
  const lighter = Math.max(firstLum, secondLum);
  const darker = Math.min(firstLum, secondLum);
  return (lighter + 0.05) / (darker + 0.05);
}

const { css: source } = await loadStyleGraph(stylesPath);
const themeBlocks = findThemeBlocks(source);
const failures = [];

for (const block of themeBlocks) {
  const vars = parseVars(block.body);
  const baseBackground = colourFor("--bg-color", vars);

  for (const check of checks) {
    const bg = colourFor(check.bg, vars, baseBackground);
    const fg = colourFor(check.fg, vars, bg ?? baseBackground);

    if (!fg || !bg) {
      failures.push(`${block.themeId}: ${check.label} could not parse ${check.fg} / ${check.bg}`);
      continue;
    }

    const ratio = contrastRatio(fg, bg);
    if (ratio < check.min) {
      failures.push(`${block.themeId}: ${check.label} is ${ratio.toFixed(2)}:1, expected at least ${check.min}:1`);
    }
  }
}

if (failures.length > 0) {
  console.error("Theme contrast check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(`Theme contrast check passed for ${themeBlocks.length} theme token blocks.`);
