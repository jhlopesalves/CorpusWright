import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { productionOutput, siteRoot } from "../scripts/paths.mjs";

const activeNavigation = new Map([
  ["quick-start.html", "Quick Start"],
  ["concepts/overview.html", "Concepts"],
  ["roadmap.html", "Roadmap"],
  ["technical-docs.html", "Docs"],
  ["about.html", "About"]
]);

test("header, footer, and metadata each have one template owner", async () => {
  const includes = path.join(siteRoot, "src", "_includes");
  for (const owner of ["header.njk", "footer.njk", "metadata.njk"]) await fs.access(path.join(includes, owner));
  const pageDirectory = path.join(siteRoot, "src", "pages");
  async function walk(directory) {
    const entries = await fs.readdir(directory, { withFileTypes: true });
    return (await Promise.all(entries.map((entry) => entry.isDirectory() ? walk(path.join(directory, entry.name)) : path.join(directory, entry.name)))).flat();
  }
  for (const page of await walk(pageDirectory)) {
    const source = await fs.readFile(page, "utf8");
    assert.doesNotMatch(source, /class="site-header"|class="site-footer"|aria-label="Primary"/, page);
  }
});

test("active navigation is generated from page data", async () => {
  for (const [route, label] of activeNavigation) {
    const html = await fs.readFile(path.join(productionOutput, route), "utf8");
    assert.match(html, new RegExp(`<a[^>]+aria-current="page"[^>]*>${label}</a>|<a[^>]*>${label}</a>`));
    const current = html.match(/<a[^>]+aria-current="page"[^>]*>([^<]+)<\/a>/g) ?? [];
    assert.equal(current.length, 1, route);
    assert.match(current[0], new RegExp(`>${label}</a>`));
  }
});

test("redirects share local hash forwarding and preserve fallback text", async () => {
  const script = await fs.readFile(path.join(productionOutput, "assets", "redirects.js"), "utf8");
  assert.match(script, /location\.replace/);
  assert.match(script, /location\.hash/);
  for (const route of ["learn.html", "method.html", "method-and-measures.html"]) {
    const html = await fs.readFile(path.join(productionOutput, route), "utf8");
    assert.match(html, /<script src="assets\/redirects\.js"><\/script>/);
    assert.match(html, /This page has moved to <a href="concepts\/overview\.html">Concepts - Overview<\/a>\./);
    assert.doesNotMatch(html, /<script>(?!\s*<\/script>)/);
  }
});
