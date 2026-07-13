export function resolveCssValue(value, declarations, trail = []) {
  return value.replace(/var\(\s*(--[\w-]+)\s*\)/g, (_match, token) => resolveTokenValue(token, declarations, trail));
}

export function resolveTokenValue(token, declarations, trail = []) {
  if (trail.includes(token)) {
    throw new Error(`CSS token alias cycle: ${[...trail, token].join(" -> ")}`);
  }
  const value = declarations.get(token);
  if (value === undefined) throw new Error(`Missing CSS colour token ${token}`);
  return resolveCssValue(value, declarations, [...trail, token]);
}

export function parseColour(value, declarations = new Map()) {
  const resolved = resolveCssValue(value.trim(), declarations).trim();
  if (resolved.toLowerCase() === "transparent") return { r: 0, g: 0, b: 0, a: 0 };

  const hexMatch = resolved.match(/^#([0-9a-f]{3,8})$/i);
  if (hexMatch) {
    const expanded = (hexMatch[1].length === 3 || hexMatch[1].length === 4)
      ? [...hexMatch[1]].map(character => character + character).join("")
      : hexMatch[1];
    if (expanded.length !== 6 && expanded.length !== 8) throw new Error(`Unparseable hex colour ${resolved}`);
    return {
      r: Number.parseInt(expanded.slice(0, 2), 16),
      g: Number.parseInt(expanded.slice(2, 4), 16),
      b: Number.parseInt(expanded.slice(4, 6), 16),
      a: expanded.length === 8 ? Number.parseInt(expanded.slice(6, 8), 16) / 255 : 1,
    };
  }

  const rgbMatch = resolved.match(/^rgba?\((.*)\)$/i);
  if (rgbMatch) return parseRgb(rgbMatch[1], resolved);

  const mixMatch = resolved.match(/^color-mix\(\s*in\s+srgb\s*,([\s\S]+)\)$/i);
  if (mixMatch) {
    const components = splitTopLevel(mixMatch[1]);
    if (components.length !== 2) throw new Error(`color-mix() requires two colours: ${resolved}`);
    const parsed = components.map(component => parseMixComponent(component, declarations));
    const firstWeight = parsed[0].weight ?? (parsed[1].weight === undefined ? 0.5 : 1 - parsed[1].weight);
    const secondWeight = parsed[1].weight ?? 1 - firstWeight;
    if (firstWeight < 0 || secondWeight < 0) throw new Error(`Invalid color-mix() percentages: ${resolved}`);
    const weightTotal = firstWeight + secondWeight;
    if (!(weightTotal > 0)) throw new Error(`Empty color-mix(): ${resolved}`);
    return mixColours(parsed[0].colour, parsed[1].colour, firstWeight / weightTotal, secondWeight / weightTotal);
  }

  throw new Error(`Unparseable CSS colour ${resolved}`);
}

export function colourForToken(token, declarations) {
  return parseColour(resolveTokenValue(token, declarations), declarations);
}

export function compositeColours(foreground, background) {
  const alpha = foreground.a + (background.a * (1 - foreground.a));
  if (alpha === 0) return { r: 0, g: 0, b: 0, a: 0 };
  return {
    r: ((foreground.r * foreground.a) + (background.r * background.a * (1 - foreground.a))) / alpha,
    g: ((foreground.g * foreground.a) + (background.g * background.a * (1 - foreground.a))) / alpha,
    b: ((foreground.b * foreground.a) + (background.b * background.a * (1 - foreground.a))) / alpha,
    a: alpha,
  };
}

export function contrastRatio(first, second) {
  const values = [relativeLuminance(first), relativeLuminance(second)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

export function deltaE76(first, second) {
  const a = rgbToLab(first);
  const b = rgbToLab(second);
  return Math.hypot(a.l - b.l, a.a - b.a, a.b - b.b);
}

function parseRgb(body, original) {
  const commaParts = splitTopLevel(body);
  let channels;
  let alpha = "1";
  if (commaParts.length >= 3) {
    channels = commaParts.slice(0, 3);
    alpha = commaParts[3] ?? "1";
  } else {
    const slashParts = body.split("/").map(part => part.trim());
    channels = slashParts[0].split(/\s+/);
    alpha = slashParts[1] ?? "1";
  }
  if (channels.length !== 3) throw new Error(`Unparseable rgb() colour ${original}`);
  const parsedChannels = channels.map(channel => channel.trim().endsWith("%")
    ? Number.parseFloat(channel) * 2.55
    : Number.parseFloat(channel));
  const parsedAlpha = alpha.trim().endsWith("%") ? Number.parseFloat(alpha) / 100 : Number.parseFloat(alpha);
  if (![...parsedChannels, parsedAlpha].every(Number.isFinite)) throw new Error(`Unparseable rgb() colour ${original}`);
  return { r: parsedChannels[0], g: parsedChannels[1], b: parsedChannels[2], a: parsedAlpha };
}

function parseMixComponent(component, declarations) {
  const percentage = component.trim().match(/^(.*\S)\s+([\d.]+)%$/);
  const colourValue = percentage ? percentage[1] : component.trim();
  return {
    colour: parseColour(colourValue, declarations),
    weight: percentage ? Number.parseFloat(percentage[2]) / 100 : undefined,
  };
}

function splitTopLevel(value) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < value.length; index += 1) {
    if (value[index] === "(") depth += 1;
    else if (value[index] === ")") depth -= 1;
    else if (value[index] === "," && depth === 0) {
      parts.push(value.slice(start, index).trim());
      start = index + 1;
    }
  }
  parts.push(value.slice(start).trim());
  return parts;
}

function mixColours(first, second, firstWeight, secondWeight) {
  const alpha = (first.a * firstWeight) + (second.a * secondWeight);
  if (alpha === 0) return { r: 0, g: 0, b: 0, a: 0 };
  return {
    r: ((first.r * first.a * firstWeight) + (second.r * second.a * secondWeight)) / alpha,
    g: ((first.g * first.a * firstWeight) + (second.g * second.a * secondWeight)) / alpha,
    b: ((first.b * first.a * firstWeight) + (second.b * second.a * secondWeight)) / alpha,
    a: alpha,
  };
}

function relativeLuminance(colour) {
  const channel = value => {
    const scaled = value / 255;
    return scaled <= 0.04045 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b);
}

function rgbToLab(colour) {
  const linear = [colour.r, colour.g, colour.b].map(value => {
    const scaled = value / 255;
    return scaled <= 0.04045 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
  });
  const x = ((linear[0] * 0.4124564) + (linear[1] * 0.3575761) + (linear[2] * 0.1804375)) / 0.95047;
  const y = (linear[0] * 0.2126729) + (linear[1] * 0.7151522) + (linear[2] * 0.072175);
  const z = ((linear[0] * 0.0193339) + (linear[1] * 0.119192) + (linear[2] * 0.9503041)) / 1.08883;
  const pivot = value => value > 0.008856 ? Math.cbrt(value) : ((7.787 * value) + (16 / 116));
  return {
    l: (116 * pivot(y)) - 16,
    a: 500 * (pivot(x) - pivot(y)),
    b: 200 * (pivot(y) - pivot(z)),
  };
}
