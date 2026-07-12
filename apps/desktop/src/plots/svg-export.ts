export function hasUnresolvedCssVariables(serializedSvg: string): boolean {
  return /var\(\s*--[\w-]+/.test(serializedSvg);
}
