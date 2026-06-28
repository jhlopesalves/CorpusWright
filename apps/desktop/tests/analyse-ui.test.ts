import { readFileSync } from "node:fs";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const workbench = readFileSync(new URL("../src/workbench.ts", import.meta.url), "utf8");

const topAnalyseMatch = html.match(
  /<div class="analyse-status-row">([\s\S]*?)<div class="analyse-workspace-shell">/,
);
assert(topAnalyseMatch, "Top Analyse status row should exist");
const topAnalyse = topAnalyseMatch[1];

assert(
  topAnalyse.includes("Build surface analysis"),
  "Top Analyse action should build surface analysis",
);
assert(
  !/annotation/i.test(topAnalyse),
  "Top Analyse area should not contain annotation controls or copy",
);
assert(
  workbench.includes('"Rebuild surface analysis"'),
  "Runtime Analyse action should rebuild surface analysis",
);
assert(
  html.includes('id="frequency-annotation-layer"'),
  "Frequency List should contain the annotation-layer mount point",
);
assert(
  !html.includes("Build annotation cache"),
  "Primary UI copy should not say Build annotation cache",
);
