import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { productionOutput, siteRoot } from "../scripts/paths.mjs";

const pages = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-copy.json"), "utf8"));
const fragments = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-fragments.json"), "utf8"));
const withoutComments = (html) => html.replace(/<!--[^]*?-->/g, "");

test("principal pages have one h1 and no skipped heading levels", async () => {
  for (const route of Object.keys(pages.pages).filter((route) => !route.includes("method") && route !== "/learn.html")) {
    const html = withoutComments(await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8"));
    const levels = [...html.matchAll(/<h([1-6])\b/gi)].map((match) => Number(match[1]));
    assert.equal(levels.filter((level) => level === 1).length, 1, route);
    for (let index = 1; index < levels.length; index += 1) {
      assert(levels[index] <= levels[index - 1] + 1, `${route}: h${levels[index - 1]} to h${levels[index]}`);
    }
  }
});

test("all baseline fragment identifiers remain present", async () => {
  for (const [route, expected] of Object.entries(fragments)) {
    const html = withoutComments(await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8"));
    const actual = new Set([...html.matchAll(/\sid=["']([^"']+)["']/gi)].map((match) => match[1]));
    for (const id of expected) assert(actual.has(id), `${route}#${id}`);
  }
});

test("content images declare geometry and valid accessibility treatment", async () => {
  for (const route of ["/index.html", "/concepts/overview.html", "/about.html"]) {
    const html = withoutComments(await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8"));
    for (const image of html.matchAll(/<img\b[^>]*>/gi)) {
      assert.match(image[0], /\bwidth=["']\d+["']/i, `${route}: ${image[0]}`);
      assert.match(image[0], /\bheight=["']\d+["']/i, `${route}: ${image[0]}`);
      assert.match(image[0], /\balt=["'][^"']*["']/i, `${route}: ${image[0]}`);
    }
  }
  const concepts = await fs.readFile(path.join(productionOutput, "concepts", "overview.html"), "utf8");
  for (const figure of concepts.matchAll(/<figure\b[^>]*>([^]*?)<\/figure>/gi)) {
    if (/<img\b/i.test(figure[1])) {
      assert.match(figure[1], /<img\b[^>]*\balt=""/i);
      assert.match(figure[1], /<figcaption>[^<]+<\/figcaption>/i);
    }
  }
});

test("Concepts remains one page and no new public HTML route appears", async () => {
  async function walk(directory) {
    const entries = await fs.readdir(directory, { withFileTypes: true });
    return (await Promise.all(entries.map((entry) => entry.isDirectory() ? walk(path.join(directory, entry.name)) : path.join(directory, entry.name)))).flat();
  }
  const html = (await walk(productionOutput))
    .filter((file) => file.endsWith(".html") && !file.includes(`${path.sep}assets${path.sep}`))
    .map((file) => `/${path.relative(productionOutput, file).split(path.sep).join("/")}`)
    .sort();
  assert.deepEqual(html, Object.keys(pages.pages).sort());
  assert.equal(html.filter((route) => route.startsWith("/concepts/")).length, 1);
});
