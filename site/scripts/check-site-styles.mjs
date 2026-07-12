import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";

async function walk(directory, extension) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = (await Promise.all(entries.map((entry) => {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) return walk(absolute, extension);
    return entry.name.endsWith(extension) ? [absolute] : [];
  }))).flat();
  return files.sort();
}

const errors = [];
const stylesRoot = path.join(siteRoot, "src", "styles");
const themeOwner = path.join(stylesRoot, "foundations", "themes.css");
const colourPattern = /#[0-9a-f]{3,8}\b|rgba?\(/i;
for (const file of await walk(stylesRoot, ".css")) {
  const css = await fs.readFile(file, "utf8");
  const relative = path.relative(siteRoot, file);
  if (file !== themeOwner && colourPattern.test(css)) errors.push(`${relative}: raw colour outside the theme owner`);
  if (/font-weight:\s*(?!400\b|600\b)\d+/i.test(css)) errors.push(`${relative}: unsupported numeric font weight`);
  if (/!important/i.test(css)) errors.push(`${relative}: undocumented !important`);
  if (/@import\s+(?:url\()?['"]?https?:/i.test(css)) errors.push(`${relative}: remote CSS import`);
}

const entry = await fs.readFile(path.join(stylesRoot, "site.css"), "utf8");
if (entry.replace(/^@import\s+[^;]+;\s*$/gm, "").trim()) errors.push("src/styles/site.css: entrypoint must contain imports only");
const generatedCss = await fs.readFile(path.join(productionOutput, "assets", "site.css"), "utf8");
if (!/font-synthesis:\s*none/i.test(generatedCss)) errors.push("generated CSS: missing font-synthesis policy");
if (/font-weight:\s*(650|700)\b/i.test(generatedCss)) errors.push("generated CSS: synthetic font weight request");

const pageStyles = path.join(stylesRoot, "pages");
for (const file of await walk(pageStyles, ".css")) {
  const css = await fs.readFile(file, "utf8");
  if (/\.(?:nav|nav-links|card|code|screenshot|docs-toc)\b/.test(css)) errors.push(`${path.relative(siteRoot, file)}: generic component selector in page owner`);
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else {
  console.log("Style ownership, colour, font-weight, synthesis, and inline-priority policies pass.");
}
