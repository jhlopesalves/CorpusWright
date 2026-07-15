import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const overview = readFileSync(
  new URL("../src/generated/CorpusOverview.ts", import.meta.url),
  "utf8",
);
const analysis = readFileSync(new URL("../src/features/analyse/ipc.ts", import.meta.url), "utf8");

const overviewTypes = [
  "AnalysisInputSource",
  "CorpusOverviewTotals",
  "CorpusOverviewPopulation",
  "CorpusOverviewDocument",
  "CorpusOverviewBalance",
  "LengthHistogramBin",
  "CorpusOverviewExtractionQuality",
  "CorpusOverviewLexicalSpectrum",
  "MattrWitness",
  "CorpusOverviewMattrDistribution",
  "CorpusOverviewFrequencyConcentration",
  "CumulativeCoveragePoint",
  "CoverageTargetCrossing",
  "FrequencySpectrumPoint",
  "RankFrequencyPoint",
  "CorpusOverviewRankFrequency",
  "VocabularyGrowthPoint",
  "CorpusOverviewVocabularyGrowth",
  "CorpusOverviewDispersion",
  "DispersionProfileBucket",
  "DispersionWitnessKind",
  "DispersionWitness",
  "CorpusOverviewDispersionPlane",
  "DispersionRosterEntry",
  "UniversalType",
  "CorpusOverviewFindingKind",
  "CorpusOverviewFindingSeverity",
  "CorpusOverviewFinding",
  "CorpusOverviewReading",
  "CorpusOverviewBuild",
];

assert.match(overview, /export type CorpusOverview = \{/);
assert.match(overview, /inputTextSource: AnalysisInputSource/);
assert.match(overview, /lexicalSpectrum: CorpusOverviewLexicalSpectrum/);
assert.match(overview, /dispersion: CorpusOverviewDispersion/);
assert.match(overview, /export type \{ AnalysisInputSource,/);

for (const name of overviewTypes) {
  const generated = readFileSync(
    new URL(`../src/generated/${name}.ts`, import.meta.url),
    "utf8",
  );
  assert.match(generated, new RegExp(`export type ${name}\\b`));
  assert.doesNotMatch(
    analysis,
    new RegExp(`interface\\s+${name}\\s*\\{`),
    `analysis.ts must not mirror generated ${name}`,
  );
}

assert.match(analysis, /generated\/CorpusOverview\.js/);
assert.match(analysis, /export type CorpusOverview = GeneratedCorpusOverview;/);
assert.doesNotMatch(
  analysis,
  /type\s+AnalysisInputSource\s*=\s*["']/,
  "AnalysisInputSource must be generated from Rust",
);
