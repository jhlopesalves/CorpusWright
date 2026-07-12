import path from "node:path";
import { findTopLevelCssBlocks, parseCompleteThemes, relativeModulePath } from "./css-contracts.mjs";
import { colourForToken, deltaE76 } from "./colour-contracts.mjs";

export const MINIMUM_RAMP_DELTA_E_76 = 6;

export function validateVisualContracts({
  modules,
  desktopRoot,
  themeSource,
  typescriptSources = [],
  exportFixtures = [],
}) {
  const violations = [];
  const allCss = modules.map(module => module.contents).join("\n");
  const report = (module, message) => violations.push(`${displayPath(module.path, desktopRoot)}: ${message}`);

  for (const module of modules) {
    if (/\bstroke-color\s*:/.test(module.contents)) report(module, "invalid stroke-color declaration");
    if (/outline\s*:[^;{}]*var\(--cw-interaction-focus-ring\)/.test(module.contents)) {
      report(module, "focus-shadow token used in outline");
    }
    if (/box-shadow\s*:[^;{}]*var\(--cw-interaction-focus-outline\)/.test(module.contents)) {
      report(module, "focus-outline token used as box shadow");
    }

    const isTypographyOwner = module.path.endsWith(`${path.sep}foundations${path.sep}base.css`);
    if (!isTypographyOwner && /\bfont-size\s*:[^;{}]*\b\d*\.?\d+(?:px|rem|em|pt|%|vw|vh)\b/.test(module.contents)) {
      report(module, "raw numeric font-size outside the typography owner");
    }
    if (!isTypographyOwner && /\bfont\s*:[^;{}]*\b\d*\.?\d+(?:px|rem|em|pt|%|vw|vh)\b/.test(module.contents)) {
      report(module, "raw numeric font shorthand outside the typography owner");
    }
    const isFontFaceOwner = module.path.endsWith(`${path.sep}foundations${path.sep}fonts.css`);
    if (!isFontFaceOwner && /\bfont-weight\s*:\s*[1-9]\d{2}\s*;/.test(module.contents)) {
      report(module, "numeric interface font-weight outside @font-face");
    }
  }

  const singleOwnerSelectors = [
    ".cw-plot-point.is-excluded",
    ...Array.from({ length: 6 }, (_, index) => `.cw-plot-ramp-${index}`),
  ];
  for (const selector of singleOwnerSelectors) {
    const count = modules.flatMap(module => findTopLevelCssBlocks(module.contents))
      .flatMap(block => block.selector.split(",").map(part => part.trim()))
      .filter(candidate => candidate === selector)
      .length;
    if (count !== 1) violations.push(`style graph: ${selector} must have exactly one generic owner, found ${count}`);
  }

  for (const animation of indefiniteAnimations(allCss)) {
    const witness = animation.selector.match(/\.[\w-]+/g)?.at(-1);
    if (!witness || !reducedRuleHas(allCss, witness, "animation", "none")) {
      violations.push(`style graph: indefinite animation ${animation.name} lacks a reduced-motion final state`);
    }
  }
  for (const entrance of entranceAnimations(allCss)) {
    for (const selector of entrance.selectors) {
      const witness = selector.match(/\.[\w-]+/g)?.at(-1);
      if (!witness) continue;
      if (entrance.opacity && !reducedRuleHas(allCss, witness, "opacity", "1")) {
        violations.push(`style graph: entrance animation ${entrance.name} leaves ${witness} transparent under reduced motion`);
      }
      if (entrance.dash && !reducedRuleHas(allCss, witness, "stroke-dashoffset", "0")) {
        violations.push(`style graph: entrance animation ${entrance.name} leaves ${witness} undrawn under reduced motion`);
      }
    }
  }

  for (const source of typescriptSources) {
    if (/FIGURE_(?:TITLE|SUBTITLE|STATS|DETAIL|READING)_FONT_SIZE/.test(source.contents)) {
      violations.push(`${displayPath(source.path, desktopRoot)}: export identity typography constant in TypeScript`);
    }
    if (/font-weight["']?\s*[,=:]\s*["']?650\b|\b650\b/.test(source.contents)) {
      violations.push(`${displayPath(source.path, desktopRoot)}: unapproved intermediate weight 650`);
    }
  }

  for (const fixture of exportFixtures) {
    if (/var\(\s*--[\w-]+/.test(fixture.contents)) {
      violations.push(`${fixture.path}: unresolved CSS variable in export regression fixture`);
    }
  }

  const staticStart = allCss.indexOf(".cw-plot-svg.cw-plot-export-static");
  const staticContract = staticStart >= 0 ? allCss.slice(staticStart, staticStart + 900) : "";
  for (const contract of ["animation: none", "transition: none", "opacity: 1", "stroke-dashoffset: 0", "transform: none"]) {
    if (!staticContract.includes(contract)) {
      violations.push(`style graph: export-static contract is missing ${contract}`);
    }
  }
  const exportSource = typescriptSources.map(source => source.contents).join("\n");
  for (const [witness, label] of [
    ['classList.remove("is-hovering")', "hover-state normalisation"],
    [".cw-plot-crosshair-x", "crosshair removal"],
    [".cw-plot-tooltip", "tooltip removal"],
    ["--cw-plot-stagger", "animation-delay sanitisation"],
    ["hasUnresolvedCssVariables", "unresolved-variable rejection"],
  ]) {
    if (typescriptSources.length > 0 && !exportSource.includes(witness)) {
      violations.push(`export sources: missing ${label}`);
    }
  }

  if (themeSource) {
    let themes = [];
    try {
      themes = parseCompleteThemes(themeSource, "foundations/themes.css");
    } catch (error) {
      violations.push(error instanceof Error ? error.message : String(error));
    }
    if (themes.length !== 16) violations.push(`theme contracts: expected 16 themes, found ${themes.length}`);
    for (const theme of themes) {
      const colours = [];
      for (let index = 0; index < 6; index += 1) {
        try {
          colours.push(colourForToken(`--cw-plot-ramp-${index}`, theme.declarations));
        } catch (error) {
          violations.push(`${theme.id}: could not parse plot ramp ${index}: ${error instanceof Error ? error.message : String(error)}`);
        }
      }
      if (colours.length !== 6) continue;
      const values = colours.map(colour => `${colour.r.toFixed(3)},${colour.g.toFixed(3)},${colour.b.toFixed(3)},${colour.a.toFixed(3)}`);
      if (new Set(values).size !== 6) violations.push(`${theme.id}: exact duplicate plot-ramp steps`);
      for (let index = 1; index < colours.length; index += 1) {
        const distance = deltaE76(colours[index - 1], colours[index]);
        if (distance < MINIMUM_RAMP_DELTA_E_76) {
          violations.push(`${theme.id}: plot ramps ${index - 1}/${index} Delta E 76 is ${distance.toFixed(2)}, expected at least ${MINIMUM_RAMP_DELTA_E_76}`);
        }
      }
    }
  }

  return violations;
}

function indefiniteAnimations(css) {
  const animations = [];
  for (const match of css.matchAll(/([^{}]+)\{([^{}]*\banimation\s*:\s*([\w-]+)[^;{}]*\binfinite\b[^{}]*)\}/g)) {
    animations.push({ selector: match[1].trim(), name: match[3] });
  }
  return animations;
}

function entranceAnimations(css) {
  const animations = [];
  for (const match of css.matchAll(/([^{}]+)\{([^{}]*\banimation\s*:\s*([\w-]+)[^;{}]*;[^{}]*)\}/g)) {
    if (/\binfinite\b/.test(match[2])) continue;
    const opacity = /opacity\s*:\s*0\s*;/.test(match[2]);
    const dash = /stroke-dashoffset\s*:\s*1\s*;/.test(match[2]);
    if (opacity || dash) {
      animations.push({
        selectors: match[1].split(",").map(selector => selector.trim()),
        name: match[3],
        opacity,
        dash,
      });
    }
  }
  return animations;
}

function reducedRuleHas(css, witness, property, value) {
  const reducedStart = css.indexOf("@media (prefers-reduced-motion: reduce)");
  if (reducedStart === -1) return false;
  const reducedCss = css.slice(reducedStart);
  return new RegExp(`${escapeRegExp(witness)}[^{}]*\\{[^}]*${escapeRegExp(property)}\\s*:\\s*${escapeRegExp(value)}(?:\\s|;)`, "s").test(reducedCss);
}

function displayPath(modulePath, desktopRoot) {
  return desktopRoot ? relativeModulePath(desktopRoot, modulePath) : modulePath;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
