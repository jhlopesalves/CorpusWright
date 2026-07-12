import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";

const baseline = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "page-metadata.json"), "utf8"));
const principalRoutes = Object.keys(baseline).filter((route) => !["/learn.html", "/method.html", "/method-and-measures.html"].includes(route));
const attribute = (tag, name) => tag.match(new RegExp(`\\s${name}=["']([^"']*)["']`, "i"))?.[1] ?? "";
let failed = false;

for (const route of principalRoutes) {
  const html = await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8");
  const metas = [...html.matchAll(/<meta\b[^>]*>/gi)].map((match) => match[0]);
  const links = [...html.matchAll(/<link\b[^>]*>/gi)].map((match) => match[0]);
  const title = html.match(/<title>([^<]+)<\/title>/i)?.[1] ?? "";
  const description = attribute(metas.find((tag) => attribute(tag, "name") === "description") ?? "", "content");
  const canonical = attribute(links.find((tag) => attribute(tag, "rel") === "canonical") ?? "", "href");
  const expectedCanonical = baseline[route].openGraph["og:url"];
  for (const [label, actual, expected] of [["title", title, baseline[route].title], ["description", description, baseline[route].description], ["canonical", canonical, expectedCanonical]]) {
    if (actual !== expected) {
      failed = true;
      console.error(`${route}: ${label} mismatch\nEXPECTED: ${expected}\nACTUAL:   ${actual}`);
    }
  }
}

if (failed) process.exitCode = 1;
else console.log(`Titles, descriptions, and canonical URLs match the baseline for ${principalRoutes.length} principal pages.`);
