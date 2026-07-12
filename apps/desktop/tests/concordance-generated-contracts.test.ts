import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";

const generatedQuery = readFileSync(
  new URL("../src/generated/ConcordanceQuery.ts", import.meta.url),
  "utf8",
);
const generatedResult = readFileSync(
  new URL("../src/generated/ConcordanceResult.ts", import.meta.url),
  "utf8",
);
const generatedCollocateRow = readFileSync(
  new URL("../src/generated/ConcordanceCollocateRow.ts", import.meta.url),
  "utf8",
);
const generatedPositionCounts = readFileSync(
  new URL("../src/generated/ConcordanceCollocatePositionCounts.ts", import.meta.url),
  "utf8",
);
const generatedDistributionPage = readFileSync(
  new URL("../src/generated/ConcordanceDistributionPage.ts", import.meta.url),
  "utf8",
);

assert.match(generatedQuery, /includeRows: boolean/);
assert.match(generatedQuery, /includeDistribution: boolean/);
assert.match(generatedResult, /rows: Array<ConcordanceRow> \| null/);
assert.match(generatedResult, /distribution: ConcordanceDistributionPage \| null/);
assert.match(generatedCollocateRow, /positionCounts: ConcordanceCollocatePositionCounts/);
assert.match(generatedPositionCounts, /left: Array<number>, right: Array<number>/);
assert.match(generatedDistributionPage, /maxRatePer10k: number/);
assert.doesNotMatch(generatedDistributionPage, /maxCount/);

const transportNames = [
  "ConcordanceQuery",
  "ConcordanceResult",
  "ConcordanceRow",
  "ConcordanceDistributionPage",
  "ConcordanceDistributionItem",
  "ConcordanceCollocationQuery",
  "ConcordanceCollocationResult",
  "ConcordanceCollocateRow",
  "ConcordanceCollocatePositionCounts",
  "ConcordanceProgress",
];
const sourceRoot = new URL("../src/", import.meta.url);
const sourceFiles: URL[] = [];
function collect(directory: URL): void {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === "generated") continue;
    const child = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, directory);
    if (entry.isDirectory()) collect(child);
    else if (entry.name.endsWith(".ts")) sourceFiles.push(child);
  }
}
collect(sourceRoot);

for (const file of sourceFiles) {
  const source = readFileSync(file, "utf8");
  for (const name of transportNames) {
    assert.doesNotMatch(
      source,
      new RegExp(`(?:type\\s+${name}\\s*=\\s*\\{|interface\\s+${name}\\s*\\{)`),
      `${file.pathname} must import the generated ${name} transport instead of mirroring it`,
    );
  }
}
