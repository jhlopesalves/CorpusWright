import type { OverviewDistributionMetric } from "./OverviewDistributionMetric.js";
import type { OverviewDistributionScale } from "./OverviewDistributionScale.js";
import type { OverviewHistogramRule } from "./OverviewHistogramRule.js";
import type { OverviewDistributionBin } from "./OverviewDistributionBin.js";
import type { OverviewEcdfPoint } from "./OverviewEcdfPoint.js";
import type { OverviewQqReference } from "./OverviewQqReference.js";
import type { OverviewQqPoint } from "./OverviewQqPoint.js";
import type { OverviewDistributionWitness } from "./OverviewDistributionWitness.js";

export type OverviewDistributionResult = { metric: OverviewDistributionMetric, xScale: OverviewDistributionScale, requestedRule: OverviewHistogramRule, effectiveRule: OverviewHistogramRule, effectiveBinCount: number, population: number, excludedCount: number, minimum: number | null, firstQuartile: number | null, median: number | null, thirdQuartile: number | null, maximum: number | null, histogramBins: Array<OverviewDistributionBin>, ecdfPoints: Array<OverviewEcdfPoint>, ecdfTotalObservations: number, ecdfTotalUniqueValues: number, ecdfCompressed: boolean, qqReference: OverviewQqReference, qqPoints: Array<OverviewQqPoint>, qqLineIntercept: number | null, qqLineSlope: number | null, qqCompressed: boolean, lowWitnesses: Array<OverviewDistributionWitness>, highWitnesses: Array<OverviewDistributionWitness>, };
