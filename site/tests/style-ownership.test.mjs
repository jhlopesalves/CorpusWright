import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { productionOutput, siteRoot } from "../scripts/paths.mjs";

const pages = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-copy.json"), "utf8"));

test("public pages have no inline style or behaviour ownership", async () => {
  for (const route of Object.keys(pages.pages)) {
    const html = (await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8")).replace(/<!--[^]*?-->/g, "");
    assert.doesNotMatch(html, /<style\b|\sstyle=["']/i, route);
    for (const script of html.matchAll(/<script\b([^>]*)>/gi)) assert.match(script[1], /\bsrc=["'][^"']+["']/i, route);
  }
});

test("owned interaction bundles and local statistical runtimes are generated", async () => {
  for (const asset of ["theme-init.js", "site.js", "mermaid.js", "mathjax-config.js", "vendor/mathjax/tex-chtml.js"]) {
    await fs.access(path.join(productionOutput, "assets", asset));
  }
});

test("the stylesheet requests only bundled real weights", async () => {
  const css = await fs.readFile(path.join(productionOutput, "assets", "site.css"), "utf8");
  assert.match(css, /font-synthesis:\s*none/);
  assert.doesNotMatch(css, /font-weight:\s*(?:650|700)\b/);
});
