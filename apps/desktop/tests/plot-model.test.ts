import {
  buildCoverageGeometry,
  createPlotBox,
  createScale,
  findFirstCoverageCrossing,
  findMilestoneShare,
  labelCollisionOffsets,
  linearTicks,
  logTicks,
  scaleValue,
} from "../src/plots/model.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function assertClose(actual: number, expected: number, message: string): void {
  assert(Math.abs(actual - expected) < 1e-9, `${message}: expected ${expected}, got ${actual}`);
}

const linear = createScale("linear", [0, 100], [10, 210]);
assertClose(scaleValue(linear, 50), 110, "Linear scale should interpolate midway");

const log = createScale("log", [1, 1000], [0, 300]);
assertClose(scaleValue(log, 10), 100, "Log scale should place powers evenly");

const logTickValues = logTicks(log).map((tick) => tick.value);
for (const value of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 20, 90, 100, 1000]) {
  assert(logTickValues.includes(value), `Log ticks should include ${value}`);
}
assert(
  logTicks(log).some((tick) => tick.value === 20 && !tick.major && tick.label === ""),
  "Log ticks should mark 2..9 decade multiples as minor ticks",
);

const linearTickLabels = linearTicks(createScale("linear", [0, 1], [100, 0]), 6).map((tick) => tick.label);
assert(
  linearTickLabels.includes("0.4") && linearTickLabels.includes("1"),
  "Linear ticks should use readable round numbers",
);

const coverage = [
  { k: 1, share: 0.12 },
  { k: 2, share: 0.21 },
  { k: 5, share: 0.38 },
  { k: 10, share: 0.55 },
];
assertClose(findMilestoneShare(coverage, 5) ?? 0, 0.38, "Milestone lookup should return exact shares");
assert(findMilestoneShare(coverage, 100) === null, "Missing milestones should stay null");
assert(findFirstCoverageCrossing(coverage, 0.5)?.k === 10, "50% crossing should use first real checkpoint");

const offsets = labelCollisionOffsets([
  { x: 10, y: 10 },
  { x: 20, y: 15 },
  { x: 30, y: 40 },
], 14);
assert(offsets[0] === 0, "First label should keep its requested offset");
assert(offsets[1] > 0, "Overlapping labels should be pushed down");
assert(offsets[2] === 0, "Separated labels should not be moved");

const geometry = buildCoverageGeometry({
  points: coverage,
  milestones: [{ k: 10, share: null }],
  box: createPlotBox(420, 260),
});
assert(geometry.halfCrossing?.k === 10, "Coverage geometry should expose the half-corpus crossing");
assert(geometry.milestones.length === 1, "Coverage geometry should emit milestone dots");
