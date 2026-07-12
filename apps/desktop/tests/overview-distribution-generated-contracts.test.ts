import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const generatedQuery = readFileSync(
  new URL("../src/generated/OverviewDistributionQuery.ts", import.meta.url),
  "utf8",
);
const generatedResult = readFileSync(
  new URL("../src/generated/OverviewDistributionResult.ts", import.meta.url),
  "utf8",
);
const generatedEvidence = readFileSync(
  new URL("../src/generated/OverviewDistributionEvidencePage.ts", import.meta.url),
  "utf8",
);
const generatedCoverage = readFileSync(
  new URL("../src/generated/CoverageTargetCrossing.ts", import.meta.url),
  "utf8",
);
const analysis = readFileSync(new URL("../src/analysis.ts", import.meta.url), "utf8");

assert.match(generatedQuery, /histogramRule: OverviewHistogramRule/);
assert.match(generatedQuery, /maximumReturnedPointCount: number/);
assert.match(generatedResult, /histogramBins: Array<OverviewDistributionBin>/);
assert.match(generatedResult, /ecdfCompressed: boolean/);
assert.match(generatedResult, /qqLineSlope: number \| null/);
assert.match(generatedEvidence, /items: Array<OverviewDistributionEvidenceRow>/);
assert.match(generatedCoverage, /firstRank: number/);
assert.match(generatedCoverage, /actualCumulativeShare: number/);

for (const name of [
  "OverviewDistributionQuery",
  "OverviewDistributionResult",
  "OverviewDistributionEvidenceQuery",
  "OverviewDistributionEvidencePage",
  "CoverageTargetCrossing",
]) {
  assert.match(
    analysis,
    new RegExp(`generated/${name}\\.js`),
    `analysis.ts must consume generated ${name}`,
  );
  assert.doesNotMatch(
    analysis,
    new RegExp(`(?:interface\\s+${name}\\s*\\{|type\\s+${name}\\s*=\\s*\\{)`),
    `analysis.ts must not mirror generated ${name}`,
  );
}

assert.match(analysis, /query_overview_distribution_command/);
assert.match(analysis, /query_overview_distribution_evidence_command/);
