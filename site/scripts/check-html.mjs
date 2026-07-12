import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { HtmlValidate } from "html-validate";
import { inspectHtml } from "./quality-rules.mjs";
import { productionOutput, siteRoot } from "./paths.mjs";

const routes = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-copy.json"), "utf8"));
const redirectRoutes = new Set(["/learn.html", "/method.html", "/method-and-measures.html"]);
const validator = new HtmlValidate({
  extends: ["html-validate:recommended"],
  rules: {
    "attr-quotes": "off",
    "doctype-style": "off",
    "no-inline-style": "error",
    "prefer-native-element": "off",
    "tel-non-breaking": "off"
  }
});
const errors = [];
for (const route of Object.keys(routes.pages)) {
  const html = await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8");
  errors.push(...inspectHtml(html, { principal: !redirectRoutes.has(route) }).map((error) => `${route}: ${error}`));
  const validationHtml = html.replace(/<pre class="mermaid">[^]*?<\/pre>/g, '<pre class="mermaid">flowchart</pre>');
  const report = await validator.validateString(validationHtml, route);
  for (const result of report.results) {
    for (const message of result.messages) errors.push(`${route}:${message.line}:${message.column} ${message.ruleId}: ${message.message}`);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else console.log(`Validator, structure, headings, IDs, accessible names, image, and ARIA checks pass for ${Object.keys(routes.pages).length} routes.`);
