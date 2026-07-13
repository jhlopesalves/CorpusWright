import type { OverviewDistributionMetric } from "./OverviewDistributionMetric.js";
import type { OverviewHistogramRule } from "./OverviewHistogramRule.js";
import type { OverviewDistributionScale } from "./OverviewDistributionScale.js";
import type { OverviewQqReference } from "./OverviewQqReference.js";

export type OverviewDistributionQuery = { metric: OverviewDistributionMetric, histogramRule: OverviewHistogramRule, manualBinCount: number, xScale: OverviewDistributionScale, qqReference: OverviewQqReference, maximumReturnedPointCount: number, };
