export type TextWidthMeasure = (value: string) => number;

export interface SvgViewBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SvgBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function wrapSvgText(
  value: string,
  maxWidth: number,
  measure: TextWidthMeasure,
): string[] {
  if (!(maxWidth > 0)) throw new RangeError("SVG text width must be positive");
  const words = value.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];

  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current ? `${current} ${word}` : word;
    if (measure(candidate) <= maxWidth) {
      current = candidate;
      continue;
    }
    if (current) {
      lines.push(current);
      current = "";
    }
    if (measure(word) <= maxWidth) {
      current = word;
      continue;
    }
    const segments = splitLongToken(word, maxWidth, measure);
    lines.push(...segments.slice(0, -1));
    current = segments[segments.length - 1] ?? "";
  }
  if (current) lines.push(current);
  return lines;
}

export function expandViewBoxForContent(
  plot: SvgViewBox,
  contentBounds: SvgBounds[],
  padding: number,
): SvgViewBox {
  const plotBottom = plot.y + plot.height;
  const contentTop = contentBounds.length > 0
    ? Math.min(...contentBounds.map(bounds => bounds.y - padding))
    : plot.y;
  const contentBottom = contentBounds.length > 0
    ? Math.max(...contentBounds.map(bounds => bounds.y + bounds.height + padding))
    : plotBottom;
  const y = Math.min(plot.y, contentTop);
  const bottom = Math.max(plotBottom, contentBottom);
  return { x: plot.x, y, width: plot.width, height: bottom - y };
}

function splitLongToken(
  token: string,
  maxWidth: number,
  measure: TextWidthMeasure,
): string[] {
  const segments: string[] = [];
  let segment = "";
  for (const character of Array.from(token)) {
    const candidate = `${segment}${character}`;
    if (segment && measure(candidate) > maxWidth) {
      segments.push(segment);
      segment = character;
    } else {
      segment = candidate;
    }
  }
  if (segment) segments.push(segment);
  return segments;
}
