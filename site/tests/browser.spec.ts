import { createRequire } from "node:module";
import { expect, test } from "@playwright/test";

const require = createRequire(import.meta.url);
const axePath = require.resolve("axe-core/axe.min.js");
const principalRoutes = [
  "/index.html",
  "/quick-start.html",
  "/concepts/overview.html",
  "/roadmap.html",
  "/technical-docs.html",
  "/citation.html",
  "/about.html",
  "/404.html"
];

test("principal pages load without console or runtime errors", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
  page.on("pageerror", (error) => errors.push(error.message));
  for (const route of principalRoutes) {
    await page.goto(route, { waitUntil: "networkidle" });
    await expect(page.locator("body")).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test("theme selection persists after reload", async ({ page }) => {
  await page.goto("/index.html");
  await page.evaluate(() => localStorage.setItem("theme", "light"));
  await page.reload();
  await page.locator("#theme-toggle").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("#theme-toggle")).toHaveAttribute("aria-pressed", "true");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(await page.evaluate(() => localStorage.getItem("theme"))).toBe("dark");
});

test("screenshot tabs support pointer and complete keyboard navigation", async ({ page }) => {
  await page.goto("/index.html");
  await page.locator("#tab-1").click();
  await expect(page.locator("#tab-1")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#panel-1")).toBeVisible();
  await page.locator("#tab-1").press("ArrowRight");
  await expect(page.locator("#tab-2")).toHaveAttribute("aria-selected", "true");
  await page.locator("#tab-2").press("ArrowLeft");
  await expect(page.locator("#tab-1")).toHaveAttribute("aria-selected", "true");
  await page.locator("#tab-1").press("Home");
  await expect(page.locator("#tab-0")).toHaveAttribute("aria-selected", "true");
  await page.locator("#tab-0").press("End");
  await expect(page.locator("#tab-2")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#panel-0")).toBeHidden();
});

test("Concepts TOC links navigate and expose active state", async ({ page }) => {
  await page.goto("/concepts/overview.html", { waitUntil: "networkidle" });
  const link = page.locator('.docs-toc a[href="#the-frequency-distribution-zipfs-law"]');
  await link.click();
  await expect(page).toHaveURL(/#the-frequency-distribution-zipfs-law$/);
  await expect(link).toHaveAttribute("aria-current", "true");
  await expect(page.locator("#the-frequency-distribution-zipfs-law")).toBeInViewport();
});

test("legacy redirects forward fragments and retain a fallback", async ({ page }) => {
  await page.goto("/learn.html#entropy-shannon-and-normalised", { waitUntil: "networkidle" });
  await expect(page).toHaveURL(/concepts\/overview\.html#entropy-shannon-and-normalised$/);
  const response = await page.request.get("/method.html");
  const source = await response.text();
  expect(source).toContain('This page has moved to <a href="concepts/overview.html">Concepts - Overview</a>.');
  expect(source).toContain('<meta http-equiv="refresh" content="0; url=concepts/overview.html">');
});

test("supported desktop and narrow viewports do not overflow horizontally", async ({ browser }) => {
  for (const viewport of [{ width: 1440, height: 900 }, { width: 1024, height: 768 }, { width: 390, height: 844 }]) {
    const context = await browser.newContext({ viewport });
    const page = await context.newPage();
    for (const route of principalRoutes) {
      await page.goto(route, { waitUntil: "networkidle" });
      const dimensions = await page.evaluate(() => ({ scroll: document.documentElement.scrollWidth, client: document.documentElement.clientWidth }));
      expect(dimensions.scroll, `${route} at ${viewport.width}x${viewport.height}`).toBeLessThanOrEqual(dimensions.client + 1);
    }
    await context.close();
  }
});

test("reduced-motion mode retains working state changes", async ({ browser }) => {
  const context = await browser.newContext({ reducedMotion: "reduce" });
  const page = await context.newPage();
  await page.goto("/index.html");
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).scrollBehavior)).toBe("auto");
  await page.locator("#tab-2").click();
  await expect(page.locator("#panel-2")).toBeVisible();
  await context.close();
});

test("JavaScript-disabled pages retain essential prose, figures, formula source, and a screenshot", async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 1440, height: 900 } });
  const concepts = await context.newPage();
  await concepts.goto("/concepts/overview.html", { waitUntil: "domcontentloaded" });
  await expect(concepts.locator("h1")).toHaveText("Overview");
  expect(await concepts.locator(".learn-prose p").count()).toBeGreaterThan(20);
  await expect(concepts.locator("pre.mermaid").first()).toContainText("flowchart TD");
  await expect(concepts.locator(".math-display").first()).toContainText("normalised frequency");
  const home = await context.newPage();
  await home.goto("/index.html", { waitUntil: "domcontentloaded" });
  await expect(home.locator("#panel-0 .theme-img-light img")).toBeVisible();
  await context.close();
});

test("principal pages have no serious or critical axe violations", async ({ page }) => {
  const failures: string[] = [];
  for (const route of principalRoutes) {
    await page.goto(route, { waitUntil: "networkidle" });
    await page.addScriptTag({ path: axePath });
    const violations = await page.evaluate(async () => {
      const result = await (window as typeof window & { axe: { run: (context?: unknown, options?: unknown) => Promise<{ violations: Array<{ id: string; impact: string | null; description: string }> }> } }).axe.run(document, {
        runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] }
      });
      return result.violations.filter((violation) => violation.impact === "serious" || violation.impact === "critical");
    });
    for (const violation of violations) failures.push(`${route}: ${violation.id} (${violation.impact}) ${violation.description}`);
  }
  expect(failures).toEqual([]);
});
