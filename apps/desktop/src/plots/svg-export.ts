/** Validates and normalises computed paint for portable standalone SVG output. */
export function hasUnresolvedCssVariables(serializedSvg: string): boolean {
  return /var\(\s*--[\w-]+/.test(serializedSvg);
}

export function hasNonPortableSvgPaint(serializedSvg: string): boolean {
  return /color-mix\s*\(|color\(\s*srgb\b/i.test(serializedSvg);
}

const SRGB_CHANNEL = "([+-]?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:e[+-]?\\d+)?%?)";
const SRGB_PAINT = new RegExp(
  `^color\\(\\s*srgb\\s+${SRGB_CHANNEL}\\s+${SRGB_CHANNEL}\\s+${SRGB_CHANNEL}(?:\\s*\\/\\s*${SRGB_CHANNEL})?\\s*\\)$`,
  "i",
);

export function normalizeComputedSvgPaint(value: string): string {
  const paint = value.trim();
  const match = SRGB_PAINT.exec(paint);
  if (!match) return paint;

  const channels = match.slice(1, 4).map(channel => Math.round(clampUnitInterval(parseSrgbComponent(channel!)) * 255));
  const alpha = match[4] === undefined ? 1 : clampUnitInterval(parseSrgbComponent(match[4]));
  const channelList = channels.join(", ");
  if (alpha >= 1) return ["rgb", "(", channelList, ")"].join("");
  return ["rgba", "(", channelList, `, ${Number(alpha.toFixed(3))})`].join("");
}

function parseSrgbComponent(value: string): number {
  const parsed = Number.parseFloat(value);
  return value.endsWith("%") ? parsed / 100 : parsed;
}

function clampUnitInterval(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}
