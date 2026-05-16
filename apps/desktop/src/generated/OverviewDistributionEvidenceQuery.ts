import type { OverviewDistributionMetric } from "./OverviewDistributionMetric.js";

export type OverviewDistributionEvidenceQuery = { metric: OverviewDistributionMetric, lowerBound: number | null, upperBound: number | null, lowerInclusive: boolean, upperInclusive: boolean, descending: boolean, offset: number, limit: number, };
