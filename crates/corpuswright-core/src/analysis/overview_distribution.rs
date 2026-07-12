//! Exact, bounded document-distribution diagnostics for the corpus Overview.

use super::build::AnalysisCorpus;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use ts_rs::TS;

const DEFAULT_POINT_LIMIT: usize = 800;
const MIN_POINT_LIMIT: usize = 2;
const MAX_POINT_LIMIT: usize = 1_000;
const MIN_MANUAL_BINS: usize = 8;
const MAX_BINS: usize = 100;
const DEFAULT_EVIDENCE_LIMIT: usize = 50;
const MAX_EVIDENCE_LIMIT: usize = 200;
const WITNESS_LIMIT: usize = 5;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum OverviewDistributionMetric {
    DocumentLength,
    Mattr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum OverviewHistogramRule {
    Auto,
    Sturges,
    Scott,
    FreedmanDiaconis,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum OverviewDistributionScale {
    Linear,
    Log,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum OverviewQqReference {
    Normal,
    Lognormal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionQuery {
    pub metric: OverviewDistributionMetric,
    pub histogram_rule: OverviewHistogramRule,
    pub manual_bin_count: usize,
    pub x_scale: OverviewDistributionScale,
    pub qq_reference: OverviewQqReference,
    pub maximum_returned_point_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionBin {
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub count: usize,
    pub share: f64,
    pub cumulative_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewEcdfPoint {
    pub value: f64,
    pub cumulative_count: usize,
    pub cumulative_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewQqPoint {
    pub expected_quantile: f64,
    pub observed_value: f64,
    pub observation_rank: usize,
    pub plotting_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionWitness {
    pub document_id: usize,
    pub document_name: String,
    pub token_count: u64,
    pub metric_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionResult {
    pub metric: OverviewDistributionMetric,
    pub x_scale: OverviewDistributionScale,
    pub requested_rule: OverviewHistogramRule,
    pub effective_rule: OverviewHistogramRule,
    pub effective_bin_count: usize,
    pub population: usize,
    pub excluded_count: usize,
    pub minimum: Option<f64>,
    pub first_quartile: Option<f64>,
    pub median: Option<f64>,
    pub third_quartile: Option<f64>,
    pub maximum: Option<f64>,
    pub histogram_bins: Vec<OverviewDistributionBin>,
    pub ecdf_points: Vec<OverviewEcdfPoint>,
    pub ecdf_total_observations: usize,
    pub ecdf_total_unique_values: usize,
    pub ecdf_compressed: bool,
    pub qq_reference: OverviewQqReference,
    pub qq_points: Vec<OverviewQqPoint>,
    pub qq_line_intercept: Option<f64>,
    pub qq_line_slope: Option<f64>,
    pub qq_compressed: bool,
    pub low_witnesses: Vec<OverviewDistributionWitness>,
    pub high_witnesses: Vec<OverviewDistributionWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionEvidenceQuery {
    pub metric: OverviewDistributionMetric,
    pub lower_bound: Option<f64>,
    pub upper_bound: Option<f64>,
    pub lower_inclusive: bool,
    pub upper_inclusive: bool,
    pub descending: bool,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionEvidenceRow {
    pub document_id: usize,
    pub document_name: String,
    pub relative_path: String,
    pub token_count: u64,
    pub metric_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDistributionEvidencePage {
    pub items: Vec<OverviewDistributionEvidenceRow>,
    pub total_matching_documents: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Clone)]
struct MetricObservation {
    document_id: usize,
    document_name: String,
    relative_path: String,
    token_count: u64,
    value: f64,
}

pub fn query_overview_distribution(
    corpus: &AnalysisCorpus,
    query: &OverviewDistributionQuery,
) -> Result<OverviewDistributionResult, String> {
    validate_query(query)?;
    let (mut observations, excluded_count) = metric_observations(corpus, query.metric);
    observations.sort_by(compare_observations);
    let values = observations.iter().map(|row| row.value).collect::<Vec<_>>();
    let point_limit = effective_point_limit(query.maximum_returned_point_count);
    let requested_rule = query.histogram_rule;
    let histogram_rule_values = values
        .iter()
        .map(|value| transform_for_scale(*value, query.x_scale))
        .collect::<Vec<_>>();
    let (effective_rule, bin_count) = choose_bin_count(
        &histogram_rule_values,
        requested_rule,
        query.manual_bin_count,
    );
    let histogram_bins = histogram(&values, query.x_scale, bin_count);
    let (ecdf_points, ecdf_total_unique_values) = ecdf(&values, point_limit);
    let qq_observed = qq_observed_values(&values, query.metric, query.qq_reference);
    let qq_points = qq(&qq_observed, point_limit);
    let (qq_line_intercept, qq_line_slope) = robust_qq_line(&qq_observed);
    let low_witnesses = observations
        .iter()
        .take(WITNESS_LIMIT)
        .map(witness)
        .collect();
    let high_start = observations.len().saturating_sub(WITNESS_LIMIT);
    let high_witnesses = observations[high_start..]
        .iter()
        .rev()
        .map(witness)
        .collect();

    Ok(OverviewDistributionResult {
        metric: query.metric,
        x_scale: query.x_scale,
        requested_rule,
        effective_rule,
        effective_bin_count: histogram_bins.len(),
        population: values.len(),
        excluded_count,
        minimum: quantile(&values, 0.0),
        first_quartile: quantile(&values, 0.25),
        median: quantile(&values, 0.5),
        third_quartile: quantile(&values, 0.75),
        maximum: quantile(&values, 1.0),
        histogram_bins,
        ecdf_compressed: ecdf_total_unique_values > ecdf_points.len(),
        ecdf_points,
        ecdf_total_observations: values.len(),
        ecdf_total_unique_values,
        qq_reference: query.qq_reference,
        qq_compressed: qq_observed.len() > qq_points.len(),
        qq_points,
        qq_line_intercept,
        qq_line_slope,
        low_witnesses,
        high_witnesses,
    })
}

pub fn query_overview_distribution_evidence(
    corpus: &AnalysisCorpus,
    query: &OverviewDistributionEvidenceQuery,
) -> Result<OverviewDistributionEvidencePage, String> {
    if query
        .lower_bound
        .zip(query.upper_bound)
        .is_some_and(|(lower, upper)| lower > upper)
    {
        return Err("The evidence lower bound must not exceed the upper bound.".to_string());
    }
    let (observations, _) = metric_observations(corpus, query.metric);
    let mut rows = observations
        .into_iter()
        .filter(|row| in_evidence_bounds(row.value, query))
        .map(|row| OverviewDistributionEvidenceRow {
            document_id: row.document_id,
            document_name: row.document_name,
            relative_path: row.relative_path,
            token_count: row.token_count,
            metric_value: row.value,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let order = left
            .metric_value
            .total_cmp(&right.metric_value)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
            .then_with(|| left.document_id.cmp(&right.document_id));
        if query.descending {
            order.reverse()
        } else {
            order
        }
    });
    let total_matching_documents = rows.len();
    let limit = if query.limit == 0 {
        DEFAULT_EVIDENCE_LIMIT
    } else {
        query.limit.min(MAX_EVIDENCE_LIMIT)
    };
    let items = rows.into_iter().skip(query.offset).take(limit).collect();
    Ok(OverviewDistributionEvidencePage {
        items,
        total_matching_documents,
        offset: query.offset,
        limit,
    })
}

fn validate_query(query: &OverviewDistributionQuery) -> Result<(), String> {
    if query.metric == OverviewDistributionMetric::Mattr
        && query.x_scale == OverviewDistributionScale::Log
    {
        return Err("Logarithmic x scale is not supported for bounded MATTR values.".to_string());
    }
    if query.metric == OverviewDistributionMetric::Mattr
        && query.qq_reference == OverviewQqReference::Lognormal
    {
        return Err("MATTR supports the normal-reference Q-Q diagnostic only.".to_string());
    }
    Ok(())
}

fn metric_observations(
    corpus: &AnalysisCorpus,
    metric: OverviewDistributionMetric,
) -> (Vec<MetricObservation>, usize) {
    let mut excluded = 0usize;
    let observations = corpus
        .documents
        .iter()
        .filter_map(|document| {
            let value = match metric {
                OverviewDistributionMetric::DocumentLength => {
                    Some(document.summary.token_count as f64)
                }
                OverviewDistributionMetric::Mattr => {
                    document.mattr.filter(|value| value.is_finite())
                }
            };
            if value.is_none() {
                excluded += 1;
            }
            value.map(|value| MetricObservation {
                document_id: document.summary.id,
                document_name: document
                    .summary
                    .relative_path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&document.summary.relative_path)
                    .to_string(),
                relative_path: document.summary.relative_path.clone(),
                token_count: document.summary.token_count,
                value,
            })
        })
        .collect();
    (observations, excluded)
}

fn compare_observations(left: &MetricObservation, right: &MetricObservation) -> Ordering {
    left.value
        .total_cmp(&right.value)
        .then_with(|| left.relative_path.cmp(&right.relative_path))
        .then_with(|| left.document_id.cmp(&right.document_id))
}

fn witness(row: &MetricObservation) -> OverviewDistributionWitness {
    OverviewDistributionWitness {
        document_id: row.document_id,
        document_name: row.document_name.clone(),
        token_count: row.token_count,
        metric_value: row.value,
    }
}

fn in_evidence_bounds(value: f64, query: &OverviewDistributionEvidenceQuery) -> bool {
    let lower_matches = query.lower_bound.is_none_or(|lower| {
        if query.lower_inclusive {
            value >= lower
        } else {
            value > lower
        }
    });
    let upper_matches = query.upper_bound.is_none_or(|upper| {
        if query.upper_inclusive {
            value <= upper
        } else {
            value < upper
        }
    });
    lower_matches && upper_matches
}

fn effective_point_limit(requested: usize) -> usize {
    if requested == 0 {
        DEFAULT_POINT_LIMIT
    } else {
        requested.clamp(MIN_POINT_LIMIT, MAX_POINT_LIMIT)
    }
}

fn choose_bin_count(
    sorted_values: &[f64],
    requested: OverviewHistogramRule,
    manual_count: usize,
) -> (OverviewHistogramRule, usize) {
    if sorted_values.len() <= 1
        || sorted_values
            .first()
            .zip(sorted_values.last())
            .is_none_or(|(minimum, maximum)| minimum == maximum)
    {
        return (requested, usize::from(!sorted_values.is_empty()));
    }
    let calculate = |rule| match rule {
        OverviewHistogramRule::Sturges => Some(sturges_bin_count(sorted_values.len())),
        OverviewHistogramRule::Scott => scott_bin_count(sorted_values),
        OverviewHistogramRule::FreedmanDiaconis => freedman_diaconis_bin_count(sorted_values),
        OverviewHistogramRule::Manual => Some(manual_count.clamp(MIN_MANUAL_BINS, MAX_BINS)),
        OverviewHistogramRule::Auto => None,
    };
    if requested != OverviewHistogramRule::Auto {
        return (
            requested,
            calculate(requested).unwrap_or_else(|| sturges_bin_count(sorted_values.len())),
        );
    }
    for rule in [
        OverviewHistogramRule::FreedmanDiaconis,
        OverviewHistogramRule::Scott,
        OverviewHistogramRule::Sturges,
    ] {
        if let Some(count) = calculate(rule).filter(|count| *count > 0) {
            return (rule, count);
        }
    }
    (OverviewHistogramRule::Sturges, 1)
}

fn sturges_bin_count(population: usize) -> usize {
    if population == 0 {
        0
    } else {
        ((population as f64).log2() + 1.0).ceil() as usize
    }
    .min(MAX_BINS)
}

fn scott_bin_count(sorted_values: &[f64]) -> Option<usize> {
    let n = sorted_values.len();
    if n < 2 {
        return None;
    }
    let mean = sorted_values.iter().sum::<f64>() / n as f64;
    let variance = sorted_values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (n - 1) as f64;
    let width = 3.5 * variance.sqrt() / (n as f64).cbrt();
    bins_from_width(sorted_values, width)
}

fn freedman_diaconis_bin_count(sorted_values: &[f64]) -> Option<usize> {
    let iqr = quantile(sorted_values, 0.75)? - quantile(sorted_values, 0.25)?;
    let width = 2.0 * iqr / (sorted_values.len() as f64).cbrt();
    bins_from_width(sorted_values, width)
}

fn bins_from_width(sorted_values: &[f64], width: f64) -> Option<usize> {
    let range = sorted_values.last()? - sorted_values.first()?;
    if !width.is_finite() || width <= 0.0 || !range.is_finite() || range <= 0.0 {
        return None;
    }
    Some(((range / width).ceil() as usize).clamp(1, MAX_BINS))
}

fn histogram(
    sorted_values: &[f64],
    scale: OverviewDistributionScale,
    bin_count: usize,
) -> Vec<OverviewDistributionBin> {
    if sorted_values.is_empty() || bin_count == 0 {
        return Vec::new();
    }
    let transformed = sorted_values
        .iter()
        .map(|value| transform_for_scale(*value, scale))
        .collect::<Vec<_>>();
    let minimum = transformed[0];
    let maximum = transformed[transformed.len() - 1];
    if minimum == maximum || bin_count == 1 {
        return vec![OverviewDistributionBin {
            lower_bound: sorted_values[0],
            upper_bound: sorted_values[sorted_values.len() - 1],
            count: sorted_values.len(),
            share: 1.0,
            cumulative_share: 1.0,
        }];
    }
    let width = (maximum - minimum) / bin_count as f64;
    let mut counts = vec![0usize; bin_count];
    for value in &transformed {
        let index = if *value >= maximum {
            bin_count - 1
        } else {
            (((*value - minimum) / width).floor() as usize).min(bin_count - 1)
        };
        counts[index] += 1;
    }
    let mut cumulative = 0usize;
    counts
        .into_iter()
        .enumerate()
        .map(|(index, count)| {
            cumulative += count;
            let transformed_lower = minimum + width * index as f64;
            let transformed_upper = if index + 1 == bin_count {
                maximum
            } else {
                minimum + width * (index + 1) as f64
            };
            OverviewDistributionBin {
                lower_bound: inverse_scale_boundary(
                    transformed_lower,
                    scale,
                    index == 0 && sorted_values[0] <= 0.0,
                ),
                upper_bound: inverse_scale_boundary(transformed_upper, scale, false),
                count,
                share: count as f64 / sorted_values.len() as f64,
                cumulative_share: cumulative as f64 / sorted_values.len() as f64,
            }
        })
        .collect()
}

fn transform_for_scale(value: f64, scale: OverviewDistributionScale) -> f64 {
    match scale {
        OverviewDistributionScale::Linear => value,
        OverviewDistributionScale::Log => value.max(1.0).log10(),
    }
}

fn inverse_scale_boundary(
    value: f64,
    scale: OverviewDistributionScale,
    contains_zero: bool,
) -> f64 {
    if contains_zero {
        0.0
    } else {
        match scale {
            OverviewDistributionScale::Linear => value,
            OverviewDistributionScale::Log => 10f64.powf(value),
        }
    }
}

fn ecdf(sorted_values: &[f64], limit: usize) -> (Vec<OverviewEcdfPoint>, usize) {
    if sorted_values.is_empty() {
        return (Vec::new(), 0);
    }
    let mut complete = Vec::new();
    let mut index = 0usize;
    while index < sorted_values.len() {
        let value = sorted_values[index];
        let mut end = index + 1;
        while end < sorted_values.len() && sorted_values[end] == value {
            end += 1;
        }
        complete.push(OverviewEcdfPoint {
            value,
            cumulative_count: end,
            cumulative_share: end as f64 / sorted_values.len() as f64,
        });
        index = end;
    }
    let unique_count = complete.len();
    let indices = bounded_indices(unique_count, limit);
    (
        indices
            .into_iter()
            .map(|index| complete[index].clone())
            .collect(),
        unique_count,
    )
}

fn qq_observed_values(
    sorted_values: &[f64],
    metric: OverviewDistributionMetric,
    reference: OverviewQqReference,
) -> Vec<f64> {
    sorted_values
        .iter()
        .map(|value| match (metric, reference) {
            (OverviewDistributionMetric::DocumentLength, OverviewQqReference::Lognormal) => {
                value.max(1.0).log10()
            }
            _ => *value,
        })
        .collect()
}

fn qq(sorted_observed: &[f64], limit: usize) -> Vec<OverviewQqPoint> {
    let n = sorted_observed.len();
    bounded_indices(n, limit)
        .into_iter()
        .map(|index| {
            let plotting_position = (index as f64 + 0.5) / n as f64;
            OverviewQqPoint {
                expected_quantile: inverse_standard_normal(plotting_position),
                observed_value: sorted_observed[index],
                observation_rank: index + 1,
                plotting_position,
            }
        })
        .collect()
}

fn bounded_indices(total: usize, limit: usize) -> Vec<usize> {
    if total <= limit {
        return (0..total).collect();
    }
    (0..limit)
        .map(|slot| slot * (total - 1) / (limit - 1))
        .collect()
}

fn robust_qq_line(sorted_observed: &[f64]) -> (Option<f64>, Option<f64>) {
    let Some(q1) = quantile(sorted_observed, 0.25) else {
        return (None, None);
    };
    let q3 = quantile(sorted_observed, 0.75).unwrap_or(q1);
    let x1 = inverse_standard_normal(0.25);
    let x3 = inverse_standard_normal(0.75);
    let slope = (q3 - q1) / (x3 - x1);
    (Some(q1 - slope * x1), Some(slope))
}

fn quantile(sorted_values: &[f64], probability: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let position = probability.clamp(0.0, 1.0) * (sorted_values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    Some(sorted_values[lower] + (sorted_values[upper] - sorted_values[lower]) * fraction)
}

// Peter J. Acklam's deterministic rational approximation. Absolute error is below
// 1.2e-8 throughout the probabilities exercised by the bounded plotting positions.
fn inverse_standard_normal(probability: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const LOW: f64 = 0.02425;
    const HIGH: f64 = 1.0 - LOW;
    let p = probability.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
    if p < LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::test_support::corpus_for;

    #[test]
    fn sturges_matches_known_population() {
        assert_eq!(sturges_bin_count(8), 4);
        assert_eq!(sturges_bin_count(16), 5);
    }

    #[test]
    fn manual_bins_are_clamped_safely() {
        let values = [0.0, 1.0];
        assert_eq!(
            choose_bin_count(&values, OverviewHistogramRule::Manual, 2).1,
            8
        );
        assert_eq!(
            choose_bin_count(&values, OverviewHistogramRule::Manual, 500).1,
            100
        );
    }

    #[test]
    fn scott_and_freedman_diaconis_match_known_sample() {
        let values = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert_eq!(scott_bin_count(&values), Some(2));
        assert_eq!(freedman_diaconis_bin_count(&values), Some(2));
    }

    #[test]
    fn auto_fallback_is_deterministic() {
        let values = [1.0, 1.0, 1.0, 1.0, 2.0];
        assert_eq!(
            choose_bin_count(&values, OverviewHistogramRule::Auto, 20),
            (OverviewHistogramRule::Scott, 2)
        );
    }

    #[test]
    fn histogram_partitions_population_and_honours_manual_count() {
        let values = (1..=100).map(f64::from).collect::<Vec<_>>();
        let bins = histogram(&values, OverviewDistributionScale::Linear, 12);
        assert_eq!(bins.len(), 12);
        assert_eq!(
            bins.iter().map(|bin| bin.count).sum::<usize>(),
            values.len()
        );
        assert_eq!(bins.last().map(|bin| bin.cumulative_share), Some(1.0));
    }

    #[test]
    fn log_histogram_preserves_zero_and_positive_observations() {
        let values = [0.0, 1.0, 10.0, 100.0];
        let bins = histogram(&values, OverviewDistributionScale::Log, 3);
        assert_eq!(bins[0].lower_bound, 0.0);
        assert_eq!(bins.iter().map(|bin| bin.count).sum::<usize>(), 4);
    }

    #[test]
    fn degenerate_distribution_is_one_truthful_bin() {
        let bins = histogram(&[3.0, 3.0, 3.0], OverviewDistributionScale::Linear, 8);
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].count, 3);
        assert_eq!(bins[0].share, 1.0);
    }

    #[test]
    fn ecdf_is_monotone_tie_aware_and_keeps_endpoints() {
        let values = [1.0, 1.0, 2.0, 4.0, 8.0];
        let (points, unique) = ecdf(&values, 3);
        assert_eq!(unique, 4);
        assert_eq!(points.first().map(|point| point.value), Some(1.0));
        assert_eq!(points.first().map(|point| point.cumulative_count), Some(2));
        assert_eq!(points.last().map(|point| point.value), Some(8.0));
        assert!(
            points
                .windows(2)
                .all(|pair| pair[0].cumulative_share <= pair[1].cumulative_share)
        );
    }

    #[test]
    fn ecdf_compression_is_deterministic() {
        let values = (0..100).map(f64::from).collect::<Vec<_>>();
        assert_eq!(ecdf(&values, 7), ecdf(&values, 7));
    }

    #[test]
    fn qq_uses_required_plotting_positions_and_is_monotone() {
        let points = qq(&[10.0, 20.0, 30.0, 40.0], 10);
        assert_eq!(points[0].plotting_position, 0.125);
        assert_eq!(points[3].plotting_position, 0.875);
        assert!(
            points
                .windows(2)
                .all(|pair| pair[0].expected_quantile < pair[1].expected_quantile)
        );
        assert!(
            points
                .windows(2)
                .all(|pair| pair[0].observed_value <= pair[1].observed_value)
        );
    }

    #[test]
    fn inverse_normal_matches_documented_reference_values() {
        assert!((inverse_standard_normal(0.5) - 0.0).abs() < 1e-12);
        assert!((inverse_standard_normal(0.975) - 1.959_963_984_540_054).abs() < 1.2e-8);
        assert!((inverse_standard_normal(0.025) + 1.959_963_984_540_054).abs() < 1.2e-8);
    }

    #[test]
    fn evidence_bounds_obey_inclusive_policy() {
        let base = OverviewDistributionEvidenceQuery {
            metric: OverviewDistributionMetric::DocumentLength,
            lower_bound: Some(10.0),
            upper_bound: Some(20.0),
            lower_inclusive: true,
            upper_inclusive: false,
            descending: false,
            offset: 0,
            limit: 20,
        };
        assert!(in_evidence_bounds(10.0, &base));
        assert!(in_evidence_bounds(19.0, &base));
        assert!(!in_evidence_bounds(20.0, &base));
    }

    #[test]
    fn bounded_indices_are_deterministic_and_keep_endpoints() {
        let indices = bounded_indices(100, 8);
        assert_eq!(indices.first(), Some(&0));
        assert_eq!(indices.last(), Some(&99));
        assert_eq!(indices, bounded_indices(100, 8));
    }

    #[test]
    fn mattr_population_excludes_ineligible_documents_exactly() {
        let long = (0..120)
            .map(|index| format!("word{}", index % 30))
            .collect::<Vec<_>>()
            .join(" ");
        let corpus = corpus_for(&[("long.txt", &long), ("short.txt", "one two three")]);
        let result = query_overview_distribution(
            &corpus,
            &OverviewDistributionQuery {
                metric: OverviewDistributionMetric::Mattr,
                histogram_rule: OverviewHistogramRule::Auto,
                manual_bin_count: 20,
                x_scale: OverviewDistributionScale::Linear,
                qq_reference: OverviewQqReference::Normal,
                maximum_returned_point_count: 800,
            },
        )
        .unwrap();
        assert_eq!(result.population, 1);
        assert_eq!(result.excluded_count, 1);
        assert_eq!(
            result
                .histogram_bins
                .iter()
                .map(|bin| bin.count)
                .sum::<usize>(),
            1
        );
    }

    #[test]
    fn evidence_query_paginates_in_deterministic_metric_order() {
        let corpus = corpus_for(&[
            ("c.txt", "one two three"),
            ("a.txt", "one"),
            ("b.txt", "one two"),
        ]);
        let page = query_overview_distribution_evidence(
            &corpus,
            &OverviewDistributionEvidenceQuery {
                metric: OverviewDistributionMetric::DocumentLength,
                lower_bound: Some(1.0),
                upper_bound: Some(3.0),
                lower_inclusive: true,
                upper_inclusive: true,
                descending: false,
                offset: 1,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(page.total_matching_documents, 3);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].relative_path, "b.txt");
        assert_eq!(page.items[0].metric_value, 2.0);
    }

    #[test]
    fn unsupported_metric_reference_combinations_are_rejected() {
        let corpus = corpus_for(&[("one.txt", "one two three")]);
        let result = query_overview_distribution(
            &corpus,
            &OverviewDistributionQuery {
                metric: OverviewDistributionMetric::Mattr,
                histogram_rule: OverviewHistogramRule::Auto,
                manual_bin_count: 20,
                x_scale: OverviewDistributionScale::Linear,
                qq_reference: OverviewQqReference::Lognormal,
                maximum_returned_point_count: 800,
            },
        );
        assert_eq!(
            result.unwrap_err(),
            "MATTR supports the normal-reference Q-Q diagnostic only."
        );
    }
}
