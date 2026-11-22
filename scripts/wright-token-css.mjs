export function declaration({ name, value }, theme) {
  const cssValue = typeof value === "string" ? value : value[theme];
  const resolved = cssValue.replace(/\{([\w-]+)\}/g, "var(--$1)");
  return `--${name}: ${resolved};`;
}

export function block(selector, declarations, indent = "") {
  return [
    `${indent}${selector} {`,
    ...declarations.map((line) => `${indent}  ${line}`),
    `${indent}}`,
  ].join("\n");
}

export function typeDeclarations(groups) {
  return groups.flatMap(({ styles }) =>
    styles.flatMap(({ name, fontSize, lineHeight, fontWeight, letterSpacing }) => [
      declaration({ name: `type-${name}-font-size`, value: fontSize }),
      declaration({ name: `type-${name}-line-height`, value: String(lineHeight) }),
      declaration({ name: `type-${name}-font-weight`, value: String(fontWeight) }),
      ...(letterSpacing ? [declaration({ name: `type-${name}-letter-spacing`, value: letterSpacing })] : []),
    ]),
  );
}
