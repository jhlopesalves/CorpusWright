import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  annotationCandidates,
  layoutBoxesOverlap,
  scoreLayoutCandidate,
} from "../src/plots/layout.ts";
import {
  buildCompositionStripGeometry,
  buildDispersionGeometry,
} from "../src/plots/model.ts";

const bounds = { x: 0, y: 0, width: 200, height: 120 };
const obstacle = { x: 40, y: 20, width: 60, height: 20 };
assert(layoutBoxesOverlap({ x: 50, y: 25, width: 20, height: 10 }, obstacle));
assert(!layoutBoxesOverlap({ x: 130, y: 80, width: 20, height: 10 }, obstacle));
assert(scoreLayoutCandidate({ x: 50, y: 25, width: 20, height: 10 }, [obstacle], bounds)
  > scoreLayoutCandidate({ x: 130, y: 80, width: 20, height: 10 }, [obstacle], bounds));
assert(scoreLayoutCandidate({ x: -10, y: 0, width: 20, height: 10 }, [], bounds) >= 1_000_000);
assert.deepEqual(annotationCandidates()[0], { dx: 0, dy: 0, displacementPenalty: 0 });

const dominantSegments = [
  { key: "dominant", label: "1 document", count: 211_271, rampIndex: 0 },
  ...Array.from({ length: 9 }, (_, index) => ({
    key: `narrow-${index}`,
    label: `${index + 2}–${index + 3} documents`,
    count: index === 8 ? 0 : 8_500 + index * 111,
    rampIndex: Math.min(5, index + 1),
  })),
];
const dominantTotal = dominantSegments.reduce((sum, segment) => sum + segment.count, 0);
const strip = buildCompositionStripGeometry({
  segments: dominantSegments,
  total: dominantTotal,
  x: 78,
  y: 18,
  width: 602,
});
assert.equal(strip.segments.length, 10);
assert.equal(strip.segments[0]?.datum.label, "1 document");
assert.equal(strip.segments[0]?.staticDetail.includes("211,271"), true);
assert.equal(strip.segments.at(-1)?.width, 0);
assert(Math.abs(strip.segments.reduce((sum, segment) => sum + segment.width, 0) - 602) < 1e-9);

const profile = buildDispersionGeometry({
  buckets: dominantSegments.map((segment, index) => ({
    key: segment.key,
    label: segment.label,
    lowerBound: index + 1,
    upperBound: index + 1,
    count: segment.count,
    share: dominantTotal > 0 ? segment.count / dominantTotal : 0,
  })),
  totalTypes: dominantTotal,
  totalDocuments: 100,
  cumulativeCallout: { count: 655, examples: ["the", "and"] },
});
assert(profile.compositionStrip);
assert.equal(profile.compositionStrip.segments.length, 10);
assert(profile.cumulativeCallout);
assert(profile.cumulativeCallout.y > profile.compositionStrip.y + profile.compositionStrip.height);
assert(profile.box.margin.top > profile.cumulativeCallout.y + profile.cumulativeCallout.height);
assert(profile.box.height < 700, "content-derived profile height must not become giant whitespace");

const layoutSource = readFileSync(new URL("../src/plots/layout.ts", import.meta.url), "utf8");
assert.match(layoutSource, /getComputedTextLength\(\)/);
assert.match(layoutSource, /getBBox\(\)/);
assert.match(layoutSource, /external-rail/);
assert.match(layoutSource, /cw-plot-label-leader is-layout-generated/);
assert.match(layoutSource, /placeOccupancyAwareLegends/);
assert.match(layoutSource, /layoutCompositionStripLabels/);
