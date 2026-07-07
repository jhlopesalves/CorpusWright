use super::build::{AnalysisBuildMetadata, AnalysisDocument, AnalysisTerm, document_name};
use super::input::AnalysisInputSource;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) const OVERVIEW_DOCUMENT_LIMIT: usize = 20;
const FREQUENT_TERM_THRESHOLD: u64 = 10;
const HIGH_DISPERSION_THRESHOLD: f64 = 0.5;
const DISPERSION_SAMPLE_TYPE_LIMIT: usize = 20;
const UNIVERSAL_TYPE_LIMIT: usize = 12;

const MAX_DISTRIBUTION_POINTS: usize = 200;

// These lexical-diversity parameters are provisional and should remain visible in the UI.
pub(super) const MATTR_WINDOW_TOKENS: usize = 100;
pub(super) const MTLD_TTR_THRESHOLD: f64 = 0.720;
pub(super) const MTLD_MIN_DOCUMENT_TOKENS: usize = 100;

// These are provisional descriptive thresholds for orientation, not inferential tests.
mod overview_thresholds {
    pub(super) const BALANCE_WORTH_A_LOOK: f64 = 0.15;
    pub(super) const BALANCE_NEEDS_FIXING: f64 = 0.25;
    pub(super) const LOW_EFFECTIVE_DOCUMENT_RATIO: f64 = 0.70;
    pub(super) const NUMERIC_WORTH_A_LOOK: f64 = 0.05;
    pub(super) const NUMERIC_NEEDS_FIXING: f64 = 0.20;
    pub(super) const DISPERSION_WORTH_A_LOOK: f64 = 0.30;
    pub(super) const DISPERSION_NEEDS_FIXING: f64 = 0.60;
    pub(super) const DEGENERATE_TYPE_COUNT: usize = 50;
}

/// Corpus totals computed during the Rust build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewTotals {
    pub tokens: u64,
    pub types: usize,
    pub documents: usize,
}

/// Project and analysis population counts available for this snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewPopulation {
    pub sources_in_project: Option<usize>,
    pub included_in_analysis: usize,
    pub processed_used: usize,
    pub using_source_text_fallback: usize,
    pub empty_or_failed: usize,
    pub sources_with_warnings: Option<usize>,
    pub input_source: String,
    pub reading: String,
}

/// One bounded document-length row in the overview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewDocument {
    pub name: String,
    pub tokens: u64,
    pub share: Option<f64>,
    pub relative_length: Option<f64>,
}

/// Document-level balance and concentration metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewBalance {
    pub largest_share: Option<f64>,
    pub top3_share: Option<f64>,
    pub gini: Option<f64>,
    pub coefficient_of_variation: Option<f64>,
    pub effective_documents: Option<f64>,
    pub median_tokens: Option<f64>,
    pub minimum_tokens: Option<u64>,
    pub maximum_tokens: Option<u64>,
    pub remaining_documents: usize,
    pub reading: String,
}

/// Type-form diagnostics computed from the indexed vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewExtractionQuality {
    pub word_like_count: usize,
    pub word_like_share: Option<f64>,
    pub numeric_only_count: usize,
    pub numeric_only_share: Option<f64>,
    pub single_character_count: Option<usize>,
    pub single_character_share: Option<f64>,
    pub reading: String,
}

/// Frequency-class partition of the indexed vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewLexicalSpectrum {
    pub types: usize,
    pub hapax_count: usize,
    pub dis_legomena_count: usize,
    pub band_3_to_9_count: usize,
    pub band_10plus_count: usize,
    pub hapax_share: Option<f64>,
    pub dis_legomena_share: Option<f64>,
    pub band_3_to_9_share: Option<f64>,
    pub band_10plus_share: Option<f64>,
    pub frequency_spectrum: Vec<FrequencySpectrumPoint>,
    pub frequency_spectrum_truncated: bool,
    pub frequency_spectrum_total: usize,
    pub mattr_window: u32,
    pub mattr_median: Option<f64>,
    pub mattr_min: Option<f64>,
    pub mattr_max: Option<f64>,
    pub mattr_docs_excluded: u32,
    pub mattr_docs_used: u32,
    pub mtld_threshold: f64,
    pub mtld_min_document_tokens: u32,
    pub mtld_median: Option<f64>,
    pub mtld_min: Option<f64>,
    pub mtld_max: Option<f64>,
    pub mtld_docs_excluded: u32,
    pub mtld_docs_used: u32,
    pub reading: String,
}

/// Token-mass coverage at vocabulary thresholds supported by this corpus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewFrequencyConcentration {
    pub top_10_coverage: Option<f64>,
    pub top_100_coverage: Option<f64>,
    pub top_1000_coverage: Option<f64>,
    pub top_10000_coverage: Option<f64>,
    pub cumulative_coverage: Vec<CumulativeCoveragePoint>,
    pub shannon_entropy_bits: Option<f64>,
    pub evenness_j: Option<f64>,
    pub effective_vocabulary_entropy: Option<f64>,
    pub reading: String,
}

/// Cumulative token share covered by the top k ranked types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CumulativeCoveragePoint {
    pub k: usize,
    pub share: f64,
}

/// Number of types observed at one exact corpus frequency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrequencySpectrumPoint {
    pub frequency: u64,
    pub type_count: usize,
}

/// One bounded point from the corpus rank-frequency distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankFrequencyPoint {
    pub rank: u64,
    pub token: Option<String>,
    pub frequency: u64,
    pub rate_per_million: f64,
}

/// Zipf fit and bounded rank-frequency series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewRankFrequency {
    pub zipf_exponent: Option<f64>,
    pub zipf_coefficient: Option<f64>,
    pub r_squared: Option<f64>,
    pub fitted_rank_start: Option<u64>,
    pub fitted_rank_end: Option<u64>,
    pub points: Vec<RankFrequencyPoint>,
    pub reading: String,
}

/// One deterministic, bounded vocabulary-growth checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyGrowthPoint {
    pub tokens: u64,
    pub types: usize,
}

/// Heaps-law fit and bounded vocabulary-growth series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewVocabularyGrowth {
    pub heaps_k: Option<f64>,
    pub heaps_beta: Option<f64>,
    pub r_squared: Option<f64>,
    pub points: Vec<VocabularyGrowthPoint>,
    pub reading: String,
}

/// Vocabulary dispersion and document-frequency summaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewDispersion {
    pub frequent_types: usize,
    pub high_dp_frequent_types: usize,
    pub median_dp: Option<f64>,
    pub df_in_1_count: usize,
    pub df_in_2_to_9_count: usize,
    pub df_in_10_to_99_count: usize,
    pub df_in_100_plus_count: usize,
    pub df_in_all_count: usize,
    pub df_in_1_sample_types: Vec<String>,
    pub df_in_2_to_9_sample_types: Vec<String>,
    pub df_in_10_to_99_sample_types: Vec<String>,
    pub df_in_100_plus_sample_types: Vec<String>,
    pub df_in_all_sample_types: Vec<String>,
    pub df_in_1_share: Option<f64>,
    pub df_in_2_to_9_share: Option<f64>,
    pub df_in_10_to_99_share: Option<f64>,
    pub df_in_100_plus_share: Option<f64>,
    pub df_in_all_share: Option<f64>,
    pub df_at_least_1pct_count: usize,
    pub df_at_least_1pct_sample_types: Vec<String>,
    pub df_at_least_1pct_share: Option<f64>,
    pub df_at_least_10pct_count: usize,
    pub df_at_least_10pct_sample_types: Vec<String>,
    pub df_at_least_10pct_share: Option<f64>,
    pub df_at_least_50pct_count: usize,
    pub df_at_least_50pct_sample_types: Vec<String>,
    pub df_at_least_50pct_share: Option<f64>,
    pub df_at_least_90pct_count: usize,
    pub df_at_least_90pct_sample_types: Vec<String>,
    pub df_at_least_90pct_share: Option<f64>,
    pub universal_types: Vec<UniversalType>,
    pub median_documents_per_type: Option<f64>,
    pub reading: String,
}

/// One type present in at least 90% of documents, capped for overview display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UniversalType {
    pub token: String,
    pub document_frequency: usize,
}

/// Overview finding category.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CorpusOverviewFindingKind {
    Balance,
    ExtractionQuality,
    LexicalSpectrum,
    Dispersion,
}

/// Descriptive attention level assigned by the overview thresholds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum CorpusOverviewFindingSeverity {
    NoIssue,
    WorthALook,
    NeedsFixing,
}

impl CorpusOverviewFindingSeverity {
    fn label(self) -> &'static str {
        match self {
            Self::NoIssue => "no immediate issue",
            Self::WorthALook => "worth a look",
            Self::NeedsFixing => "needs fixing",
        }
    }
}

/// One computed finding shown in the corpus reading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewFinding {
    pub key: CorpusOverviewFindingKind,
    pub severity: CorpusOverviewFindingSeverity,
    pub severity_label: String,
    pub text: String,
}

/// Headline interpretation derived from the computed overview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewReading {
    pub verdict: String,
    pub findings: Vec<CorpusOverviewFinding>,
}

/// Analysis build details that are known for this snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewBuild {
    pub tokenisation: Option<String>,
    pub case_handling: Option<String>,
    pub lemmatisation: Option<String>,
    pub number_punctuation_handling: Option<String>,
    pub input_source: String,
    pub config_version: Option<String>,
    pub snapshot_date: Option<String>,
    pub metadata_attached: bool,
}

/// Project metadata supplied by the desktop layer when an analysis is built.
#[derive(Debug, Clone, Default)]
pub struct CorpusOverviewContext {
    pub sources_in_project: Option<usize>,
    pub sources_with_warnings: Option<usize>,
    pub failed_sources: usize,
    pub config_version: Option<String>,
    pub metadata_attached: bool,
}

/// Render-ready first-stage corpus EDA summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverview {
    pub document_count: usize,
    pub source_count: usize,
    pub total_tokens: u64,
    pub total_types: usize,
    pub annotation_available: bool,
    pub input_text_source: AnalysisInputSource,
    pub built_at: String,
    pub build_signature: String,
    pub processing_config_signature: String,
    pub skipped_documents: usize,
    pub skipped_pdf_documents: usize,
    pub totals: CorpusOverviewTotals,
    pub population: CorpusOverviewPopulation,
    pub documents: Vec<CorpusOverviewDocument>,
    pub documents_truncated: bool,
    pub balance: CorpusOverviewBalance,
    pub extraction_quality: CorpusOverviewExtractionQuality,
    pub lexical_spectrum: CorpusOverviewLexicalSpectrum,
    pub frequency_concentration: CorpusOverviewFrequencyConcentration,
    pub rank_frequency: CorpusOverviewRankFrequency,
    pub vocabulary_growth: CorpusOverviewVocabularyGrowth,
    pub dispersion: CorpusOverviewDispersion,
    pub reading: CorpusOverviewReading,
    pub build: CorpusOverviewBuild,
}

fn percentile(sorted_values: &[u64], percentile_value: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let position = (sorted_values.len() - 1) as f64 * percentile_value;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return Some(sorted_values[lower] as f64);
    }
    let weight = position - lower as f64;
    Some(sorted_values[lower] as f64 * (1.0 - weight) + sorted_values[upper] as f64 * weight)
}

fn median_f64(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[middle - 1] + values[middle]) / 2.0)
    } else {
        Some(values[middle])
    }
}

pub(super) fn mattr(tokens: &[String], window: usize) -> Option<f64> {
    if window == 0 || tokens.len() < window {
        return None;
    }

    let mut counts = HashMap::<&str, usize>::new();
    for token in &tokens[..window] {
        *counts.entry(token.as_str()).or_default() += 1;
    }
    let mut ttr_sum = counts.len() as f64 / window as f64;
    let window_count = tokens.len() - window + 1;
    for start in 1..window_count {
        let outgoing = tokens[start - 1].as_str();
        let remove_outgoing = counts.get_mut(outgoing).is_some_and(|count| {
            *count -= 1;
            *count == 0
        });
        if remove_outgoing {
            counts.remove(outgoing);
        }
        *counts
            .entry(tokens[start + window - 1].as_str())
            .or_default() += 1;
        ttr_sum += counts.len() as f64 / window as f64;
    }
    Some(ttr_sum / window_count as f64)
}

fn mtld_direction<'a>(
    tokens: impl Iterator<Item = &'a str>,
    token_count: usize,
    threshold: f64,
) -> Option<f64> {
    if token_count == 0 || !(0.0..1.0).contains(&threshold) {
        return None;
    }

    let mut types = HashSet::new();
    let mut segment_tokens = 0usize;
    let mut factors = 0.0;
    for token in tokens {
        segment_tokens += 1;
        types.insert(token);
        let ttr = types.len() as f64 / segment_tokens as f64;
        if ttr <= threshold {
            factors += 1.0;
            types.clear();
            segment_tokens = 0;
        }
    }
    if segment_tokens > 0 {
        let ttr = types.len() as f64 / segment_tokens as f64;
        factors += (1.0 - ttr) / (1.0 - threshold);
    }
    (factors > 0.0)
        .then(|| token_count as f64 / factors)
        .filter(|value| value.is_finite())
}

pub(super) fn mtld(tokens: &[String], threshold: f64) -> Option<f64> {
    let forward = mtld_direction(tokens.iter().map(String::as_str), tokens.len(), threshold)?;
    let backward = mtld_direction(
        tokens.iter().rev().map(String::as_str),
        tokens.len(),
        threshold,
    )?;
    Some((forward + backward) / 2.0)
}

#[derive(Debug, Clone, Copy)]
struct MetricSummary {
    median: Option<f64>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    documents_used: usize,
    documents_excluded: usize,
}

fn summarise_document_metric(
    documents: &[AnalysisDocument],
    metric: impl Fn(&AnalysisDocument) -> Option<f64>,
) -> MetricSummary {
    let mut values = documents
        .iter()
        .filter_map(metric)
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let documents_used = values.len();
    let minimum = values.iter().copied().min_by(f64::total_cmp);
    let maximum = values.iter().copied().max_by(f64::total_cmp);
    let median = median_f64(&mut values);
    MetricSummary {
        median,
        minimum,
        maximum,
        documents_used,
        documents_excluded: documents.len().saturating_sub(documents_used),
    }
}

pub(super) fn entropy_metrics(
    frequencies: &[u64],
    total_tokens: u64,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    if total_tokens == 0 || frequencies.len() <= 1 {
        return (None, None, None);
    }
    let entropy = -frequencies
        .iter()
        .filter(|frequency| **frequency > 0)
        .map(|frequency| {
            let probability = *frequency as f64 / total_tokens as f64;
            probability * probability.log2()
        })
        .sum::<f64>();
    let evenness = (entropy / (frequencies.len() as f64).log2()).clamp(0.0, 1.0);
    let effective_vocabulary = 2.0f64.powf(entropy);
    if entropy.is_finite() && evenness.is_finite() && effective_vocabulary.is_finite() {
        (Some(entropy), Some(evenness), Some(effective_vocabulary))
    } else {
        (None, None, None)
    }
}

fn linear_regression(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let count = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / count;
    let ss_x = points
        .iter()
        .map(|(x, _)| (x - mean_x).powi(2))
        .sum::<f64>();
    if ss_x <= f64::EPSILON {
        return None;
    }
    let covariance = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let slope = covariance / ss_x;
    let intercept = mean_y - slope * mean_x;
    let ss_total = points
        .iter()
        .map(|(_, y)| (y - mean_y).powi(2))
        .sum::<f64>();
    let ss_residual = points
        .iter()
        .map(|(x, y)| (y - (intercept + slope * x)).powi(2))
        .sum::<f64>();
    let r_squared = if ss_total <= f64::EPSILON {
        1.0
    } else {
        (1.0 - ss_residual / ss_total).clamp(0.0, 1.0)
    };
    (slope.is_finite() && intercept.is_finite() && r_squared.is_finite())
        .then_some((slope, intercept, r_squared))
}

fn sampled_indices(length: usize, maximum: usize) -> Vec<usize> {
    if length <= maximum {
        return (0..length).collect();
    }
    (0..maximum)
        .map(|index| index * (length - 1) / (maximum - 1))
        .collect()
}

pub(super) fn rank_frequency_summary(
    frequencies: &[u64],
    tokens: &[String],
    total_tokens: u64,
) -> CorpusOverviewRankFrequency {
    let fit_points = frequencies
        .iter()
        .enumerate()
        .filter(|(index, frequency)| *index >= 3 && **frequency > 1)
        .map(|(index, frequency)| (((index + 1) as f64).ln(), (*frequency as f64).ln()))
        .collect::<Vec<_>>();
    let fit = linear_regression(&fit_points);
    let fitted_rank_start = fit.as_ref().map(|_| 4);
    let fitted_rank_end = fit.as_ref().and_then(|_| {
        frequencies
            .iter()
            .enumerate()
            .rev()
            .find(|(index, frequency)| *index >= 3 && **frequency > 1)
            .map(|(index, _)| u64::try_from(index + 1).unwrap_or(u64::MAX))
    });
    let points = sampled_indices(frequencies.len(), MAX_DISTRIBUTION_POINTS)
        .into_iter()
        .map(|index| RankFrequencyPoint {
            rank: u64::try_from(index + 1).unwrap_or(u64::MAX),
            token: tokens.get(index).cloned(),
            frequency: frequencies[index],
            rate_per_million: if total_tokens > 0 {
                frequencies[index] as f64 / total_tokens as f64 * 1_000_000.0
            } else {
                0.0
            },
        })
        .collect();
    CorpusOverviewRankFrequency {
        zipf_exponent: fit.map(|(slope, _, _)| -slope),
        zipf_coefficient: fit.map(|(_, intercept, _)| intercept.exp()),
        r_squared: fit.map(|(_, _, r_squared)| r_squared),
        fitted_rank_start,
        fitted_rank_end,
        points,
        reading: "The Zipf fit uses log frequency against log rank, excluding the three highest-frequency types and the frequency-one tail where the classic law is least reliable."
            .to_string(),
    }
}

pub(super) fn vocabulary_growth_summary(
    points: &[VocabularyGrowthPoint],
) -> CorpusOverviewVocabularyGrowth {
    let fit_points = points
        .iter()
        .filter(|point| point.tokens > 1 && point.types > 1)
        .map(|point| ((point.tokens as f64).ln(), (point.types as f64).ln()))
        .collect::<Vec<_>>();
    let fit = linear_regression(&fit_points);
    CorpusOverviewVocabularyGrowth {
        heaps_k: fit.map(|(_, intercept, _)| intercept.exp()),
        heaps_beta: fit.map(|(slope, _, _)| slope),
        r_squared: fit.map(|(_, _, r_squared)| r_squared),
        points: points.to_vec(),
        reading: "Heaps' law is fitted to deterministic, power-of-two token checkpoints in stable source-path order; it describes this snapshot rather than predicting a universal vocabulary ceiling."
            .to_string(),
    }
}

pub(super) fn frequency_spectrum(
    frequencies: &[u64],
) -> (Vec<FrequencySpectrumPoint>, bool, usize) {
    let mut counts = BTreeMap::<u64, usize>::new();
    for &frequency in frequencies {
        *counts.entry(frequency).or_default() += 1;
    }
    let all_points = counts
        .into_iter()
        .map(|(frequency, type_count)| FrequencySpectrumPoint {
            frequency,
            type_count,
        })
        .collect::<Vec<_>>();
    let truncated = all_points.len() > MAX_DISTRIBUTION_POINTS;
    let total = all_points.len();
    let points = sampled_indices(all_points.len(), MAX_DISTRIBUTION_POINTS)
        .into_iter()
        .map(|index| all_points[index].clone())
        .collect();
    (points, truncated, total)
}

pub(super) fn cumulative_coverage_checkpoints(
    frequencies: &[u64],
    total_tokens: u64,
) -> Vec<CumulativeCoveragePoint> {
    if frequencies.is_empty() || total_tokens == 0 {
        return Vec::new();
    }
    let checkpoints = coverage_checkpoint_ladder(frequencies.len());
    let mut checkpoint_index = 0usize;
    let mut cumulative = 0u64;
    let mut points = Vec::with_capacity(checkpoints.len());
    for (index, frequency) in frequencies.iter().enumerate() {
        cumulative = cumulative.saturating_add(*frequency);
        let k = index + 1;
        while checkpoint_index < checkpoints.len() && checkpoints[checkpoint_index] == k {
            points.push(CumulativeCoveragePoint {
                k,
                share: cumulative as f64 / total_tokens as f64,
            });
            checkpoint_index += 1;
        }
    }
    points
}

fn coverage_checkpoint_ladder(vocabulary_size: usize) -> Vec<usize> {
    if vocabulary_size == 0 {
        return Vec::new();
    }
    let mut checkpoints = Vec::new();
    let mut scale = 1usize;
    loop {
        let mut added_at_scale = false;
        for multiplier in [1usize, 2, 5] {
            let Some(k) = multiplier.checked_mul(scale) else {
                break;
            };
            if k > vocabulary_size {
                continue;
            }
            checkpoints.push(k);
            added_at_scale = true;
        }
        let Some(next_scale) = scale.checked_mul(10) else {
            break;
        };
        if next_scale > vocabulary_size && !added_at_scale {
            break;
        }
        if scale > vocabulary_size {
            break;
        }
        scale = next_scale;
    }
    if checkpoints.last().copied() != Some(vocabulary_size) {
        checkpoints.push(vocabulary_size);
    }
    checkpoints.sort_unstable();
    checkpoints.dedup();
    checkpoints
}

pub(super) fn universal_types(
    terms: &[&AnalysisTerm],
    document_frequency_threshold: usize,
) -> Vec<UniversalType> {
    if document_frequency_threshold == 0 {
        return Vec::new();
    }
    let mut candidates = terms
        .iter()
        .filter(|term| term.document_frequency >= document_frequency_threshold)
        .copied()
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        compare_type_sample(left.frequency, &left.token, right.frequency, &right.token)
    });
    candidates
        .into_iter()
        .take(UNIVERSAL_TYPE_LIMIT)
        .map(|term| UniversalType {
            token: term.token.clone(),
            document_frequency: term.document_frequency,
        })
        .collect()
}

pub(super) fn gini_coefficient(values: &[u64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let total = sorted.iter().sum::<u64>() as f64;
    if total == 0.0 {
        return Some(0.0);
    }
    let weighted_sum = sorted
        .iter()
        .enumerate()
        .map(|(index, value)| (index + 1) as f64 * *value as f64)
        .sum::<f64>();
    let count = sorted.len() as f64;
    Some((2.0 * weighted_sum) / (count * total) - (count + 1.0) / count)
}

pub(super) fn coefficient_of_variation(values: &[u64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().sum::<u64>() as f64 / values.len() as f64;
    if mean == 0.0 {
        return None;
    }
    let variance = values
        .iter()
        .map(|value| {
            let difference = *value as f64 - mean;
            difference * difference
        })
        .sum::<f64>()
        / values.len() as f64;
    Some(variance.sqrt() / mean)
}

pub(super) fn effective_document_count(values: &[u64], total_tokens: u64) -> Option<f64> {
    if values.is_empty() || total_tokens == 0 {
        return None;
    }
    let squared_shares = values
        .iter()
        .map(|value| {
            let share = *value as f64 / total_tokens as f64;
            share * share
        })
        .sum::<f64>();
    (squared_shares > 0.0).then(|| 1.0 / squared_shares)
}

fn numeric_only(token: &str) -> bool {
    // Separators are accepted only when every other scalar is numeric; this avoids
    // classifying mixed alphanumeric identifiers as numeric extraction debris.
    let mut has_digit = false;
    for character in token.chars() {
        if character.is_numeric() {
            has_digit = true;
        } else if !matches!(character, '.' | ',' | ':' | '-' | '/') {
            return false;
        }
    }
    has_digit
}

fn coverage_at(frequencies: &[u64], threshold: usize, total_tokens: u64) -> Option<f64> {
    if frequencies.len() < threshold || total_tokens == 0 {
        return None;
    }
    Some(frequencies.iter().take(threshold).sum::<u64>() as f64 / total_tokens as f64)
}

fn type_share(count: usize, total: usize) -> Option<f64> {
    (total > 0).then(|| count as f64 / total as f64)
}

fn document_frequency_threshold(document_count: usize, proportion: f64) -> usize {
    if document_count == 0 {
        return 0;
    }

    ((document_count as f64 * proportion).ceil() as usize).clamp(1, document_count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeSample {
    token: String,
    frequency: u64,
}

#[derive(Debug, Default)]
struct TypeSampleCollector {
    samples: Vec<TypeSample>,
}

impl TypeSampleCollector {
    fn consider(&mut self, term: &AnalysisTerm) {
        if self.samples.len() < DISPERSION_SAMPLE_TYPE_LIMIT {
            self.samples.push(TypeSample {
                token: term.token.clone(),
                frequency: term.frequency,
            });
            return;
        }

        let Some(worst_index) = self.worst_index() else {
            return;
        };
        let worst = &self.samples[worst_index];
        if compare_type_sample(term.frequency, &term.token, worst.frequency, &worst.token)
            == Ordering::Less
        {
            self.samples[worst_index] = TypeSample {
                token: term.token.clone(),
                frequency: term.frequency,
            };
        }
    }

    fn into_ordered_tokens(mut self) -> Vec<String> {
        self.samples.sort_unstable_by(|left, right| {
            compare_type_sample(left.frequency, &left.token, right.frequency, &right.token)
        });
        self.samples
            .into_iter()
            .map(|sample| sample.token)
            .collect()
    }

    fn worst_index(&self) -> Option<usize> {
        let mut worst_index = 0;
        for index in 1..self.samples.len() {
            let worst = &self.samples[worst_index];
            let candidate = &self.samples[index];
            if compare_type_sample(
                worst.frequency,
                &worst.token,
                candidate.frequency,
                &candidate.token,
            ) == Ordering::Less
            {
                worst_index = index;
            }
        }
        Some(worst_index)
    }
}

fn compare_type_sample(
    left_frequency: u64,
    left_token: &str,
    right_frequency: u64,
    right_token: &str,
) -> Ordering {
    right_frequency
        .cmp(&left_frequency)
        .then_with(|| left_token.cmp(right_token))
}

pub(super) fn balance_severity(
    document_count: usize,
    total_tokens: u64,
    largest_share: Option<f64>,
    effective_documents: Option<f64>,
) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    use overview_thresholds::{
        BALANCE_NEEDS_FIXING, BALANCE_WORTH_A_LOOK, LOW_EFFECTIVE_DOCUMENT_RATIO,
    };

    if document_count == 0 || total_tokens == 0 {
        return NeedsFixing;
    }
    if document_count == 1 {
        return WorthALook;
    }
    let mut severity = match largest_share {
        Some(value) if value > BALANCE_NEEDS_FIXING => NeedsFixing,
        Some(value) if value >= BALANCE_WORTH_A_LOOK => WorthALook,
        Some(_) => NoIssue,
        None => NeedsFixing,
    };
    if effective_documents
        .is_some_and(|value| value / (document_count as f64) < LOW_EFFECTIVE_DOCUMENT_RATIO)
    {
        severity = severity.max(WorthALook);
    }
    severity
}

pub(super) fn extraction_severity(
    total_types: usize,
    numeric_only_share: Option<f64>,
) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    use overview_thresholds::{NUMERIC_NEEDS_FIXING, NUMERIC_WORTH_A_LOOK};

    if total_types == 0 {
        return NeedsFixing;
    }
    match numeric_only_share {
        Some(value) if value > NUMERIC_NEEDS_FIXING => NeedsFixing,
        Some(value) if value >= NUMERIC_WORTH_A_LOOK => WorthALook,
        _ => NoIssue,
    }
}

fn lexical_severity(total_tokens: u64, total_types: usize) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    if total_tokens == 0 {
        NeedsFixing
    } else if total_types < overview_thresholds::DEGENERATE_TYPE_COUNT {
        WorthALook
    } else {
        NoIssue
    }
}

pub(super) fn dispersion_severity(
    document_count: usize,
    frequent_types: usize,
    median_dp: Option<f64>,
) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    use overview_thresholds::{DISPERSION_NEEDS_FIXING, DISPERSION_WORTH_A_LOOK};

    if document_count < 2 || frequent_types == 0 {
        return NoIssue;
    }
    match median_dp {
        Some(value) if value > DISPERSION_NEEDS_FIXING => NeedsFixing,
        Some(value) if value >= DISPERSION_WORTH_A_LOOK => WorthALook,
        Some(_) => NoIssue,
        None => NoIssue,
    }
}

fn finding(
    key: CorpusOverviewFindingKind,
    severity: CorpusOverviewFindingSeverity,
    text: String,
) -> CorpusOverviewFinding {
    CorpusOverviewFinding {
        key,
        severity,
        severity_label: severity.label().to_string(),
        text,
    }
}

fn analysis_input_label(processed_documents: usize, fallback_documents: usize) -> String {
    match (processed_documents, fallback_documents) {
        (0, 0) => "No usable analysis text".to_string(),
        (0, _) => "Source text fallback".to_string(),
        (_, 0) => "Processed text".to_string(),
        (_, fallback) => format!("Processed text; {fallback} using source-text fallback"),
    }
}

pub(super) fn build_overview(
    documents: &[AnalysisDocument],
    terms: &[AnalysisTerm],
    metadata: AnalysisBuildMetadata,
) -> CorpusOverview {
    let total_tokens = metadata.total_tokens;
    let mut sorted_documents = documents
        .iter()
        .map(|document| document.summary.clone())
        .collect::<Vec<_>>();
    sorted_documents.sort_by(|left, right| {
        right
            .token_count
            .cmp(&left.token_count)
            .then_with(|| document_name(left).cmp(document_name(right)))
    });
    let mut token_counts = documents
        .iter()
        .map(|document| document.summary.token_count)
        .collect::<Vec<_>>();
    token_counts.sort_unstable();
    let median_tokens = percentile(&token_counts, 0.5);
    let largest_share = sorted_documents
        .first()
        .filter(|_| total_tokens > 0)
        .map(|document| document.token_count as f64 / total_tokens as f64);
    let top3_share = (total_tokens > 0 && !sorted_documents.is_empty()).then(|| {
        sorted_documents
            .iter()
            .take(3)
            .map(|document| document.token_count)
            .sum::<u64>() as f64
            / total_tokens as f64
    });
    let gini = gini_coefficient(&token_counts);
    let coefficient_of_variation = coefficient_of_variation(&token_counts);
    let effective_documents = effective_document_count(&token_counts, total_tokens);
    let mattr_summary = summarise_document_metric(documents, |document| document.mattr);
    let mtld_summary = summarise_document_metric(documents, |document| document.mtld);
    let balance_severity = balance_severity(
        documents.len(),
        total_tokens,
        largest_share,
        effective_documents,
    );

    let mut hapax_count = 0usize;
    let mut dis_legomena = 0usize;
    let mut band_3_to_9_count = 0usize;
    let mut band_10plus_count = 0usize;
    let mut word_like_count = 0usize;
    let mut numeric_only_types = 0usize;
    let mut single_character_types = 0usize;
    for term in terms {
        match term.frequency {
            1 => hapax_count += 1,
            2 => dis_legomena += 1,
            3..=9 => band_3_to_9_count += 1,
            _ => band_10plus_count += 1,
        }
        if term.token.chars().any(char::is_alphabetic) {
            word_like_count += 1;
        }
        if numeric_only(&term.token) {
            numeric_only_types += 1;
        }
        // The analysis token representation is Unicode-scalar based; grapheme clusters
        // are not available without changing the tokeniser's established contract.
        if term.token.chars().count() == 1 {
            single_character_types += 1;
        }
    }
    let total_types = terms.len();

    let frequent_terms = terms
        .iter()
        .filter(|term| term.frequency >= FREQUENT_TERM_THRESHOLD)
        .collect::<Vec<_>>();
    let mut frequent_dispersions = if documents.len() >= 2 {
        frequent_terms
            .iter()
            .map(|term| term.dispersion)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    debug_assert_eq!(band_10plus_count, frequent_terms.len());
    let document_count = documents.len();
    let annotation_available = !documents.is_empty()
        && documents
            .iter()
            .all(|document| document.annotation_available);
    let df_at_least_1pct_threshold = document_frequency_threshold(document_count, 0.01);
    let df_at_least_10pct_threshold = document_frequency_threshold(document_count, 0.10);
    let df_at_least_50pct_threshold = document_frequency_threshold(document_count, 0.50);
    let df_at_least_90pct_threshold = document_frequency_threshold(document_count, 0.90);
    let mut df_in_1_count = 0usize;
    let mut df_in_2_to_9_count = 0usize;
    let mut df_in_10_to_99_count = 0usize;
    let mut df_in_100_plus_count = 0usize;
    let mut df_in_all_count = 0usize;
    let mut df_at_least_1pct_count = 0usize;
    let mut df_at_least_10pct_count = 0usize;
    let mut df_at_least_50pct_count = 0usize;
    let mut df_at_least_90pct_count = 0usize;
    let mut df_in_1_samples = TypeSampleCollector::default();
    let mut df_in_2_to_9_samples = TypeSampleCollector::default();
    let mut df_in_10_to_99_samples = TypeSampleCollector::default();
    let mut df_in_100_plus_samples = TypeSampleCollector::default();
    let mut df_in_all_samples = TypeSampleCollector::default();
    let mut df_at_least_1pct_samples = TypeSampleCollector::default();
    let mut df_at_least_10pct_samples = TypeSampleCollector::default();
    let mut df_at_least_50pct_samples = TypeSampleCollector::default();
    let mut df_at_least_90pct_samples = TypeSampleCollector::default();
    for term in terms {
        let df = term.document_frequency;
        debug_assert!(df > 0);
        match df {
            1 => {
                df_in_1_count += 1;
                df_in_1_samples.consider(term);
            }
            2..=9 => {
                df_in_2_to_9_count += 1;
                df_in_2_to_9_samples.consider(term);
            }
            10..=99 => {
                df_in_10_to_99_count += 1;
                df_in_10_to_99_samples.consider(term);
            }
            _ => {
                df_in_100_plus_count += 1;
                df_in_100_plus_samples.consider(term);
            }
        }
        if document_count > 0 && df == document_count {
            df_in_all_count += 1;
            df_in_all_samples.consider(term);
        }
        if df_at_least_1pct_threshold > 0 && df >= df_at_least_1pct_threshold {
            df_at_least_1pct_count += 1;
            df_at_least_1pct_samples.consider(term);
        }
        if df_at_least_10pct_threshold > 0 && df >= df_at_least_10pct_threshold {
            df_at_least_10pct_count += 1;
            df_at_least_10pct_samples.consider(term);
        }
        if df_at_least_50pct_threshold > 0 && df >= df_at_least_50pct_threshold {
            df_at_least_50pct_count += 1;
            df_at_least_50pct_samples.consider(term);
        }
        if df_at_least_90pct_threshold > 0 && df >= df_at_least_90pct_threshold {
            df_at_least_90pct_count += 1;
            df_at_least_90pct_samples.consider(term);
        }
    }
    let mut document_frequencies = terms
        .iter()
        .map(|term| term.document_frequency as u64)
        .collect::<Vec<_>>();
    document_frequencies.sort_unstable();
    let median_documents_per_type = percentile(&document_frequencies, 0.5);
    let median_dp = median_f64(&mut frequent_dispersions);
    let high_dp_frequent_types = if document_count >= 2 {
        frequent_terms
            .iter()
            .filter(|term| term.dispersion >= HIGH_DISPERSION_THRESHOLD)
            .count()
    } else {
        0
    };
    let extraction_severity =
        extraction_severity(total_types, type_share(numeric_only_types, total_types));
    let lexical_severity = lexical_severity(total_tokens, total_types);
    let dispersion_severity = dispersion_severity(document_count, frequent_terms.len(), median_dp);

    let mut ranked_terms = terms.iter().collect::<Vec<_>>();
    ranked_terms.sort_by(|left, right| {
        compare_type_sample(left.frequency, &left.token, right.frequency, &right.token)
    });
    let frequencies = ranked_terms
        .iter()
        .map(|term| term.frequency)
        .collect::<Vec<_>>();
    let rank_tokens = ranked_terms
        .iter()
        .map(|term| term.token.clone())
        .collect::<Vec<_>>();
    let top_10_coverage = coverage_at(&frequencies, 10, total_tokens);
    let top_100_coverage = coverage_at(&frequencies, 100, total_tokens);
    let top_1000_coverage = coverage_at(&frequencies, 1_000, total_tokens);
    let top_10000_coverage = coverage_at(&frequencies, 10_000, total_tokens);
    let cumulative_coverage = cumulative_coverage_checkpoints(&frequencies, total_tokens);
    let (shannon_entropy_bits, evenness_j, effective_vocabulary_entropy) =
        entropy_metrics(&frequencies, total_tokens);
    let (frequency_spectrum, frequency_spectrum_truncated, frequency_spectrum_total) =
        frequency_spectrum(&frequencies);
    let rank_frequency = rank_frequency_summary(&frequencies, &rank_tokens, total_tokens);
    let vocabulary_growth = vocabulary_growth_summary(&metadata.vocabulary_growth);
    let universal_types = universal_types(&ranked_terms, df_at_least_90pct_threshold);

    let sources_in_project = metadata
        .overview_context
        .sources_in_project
        .or(Some(metadata.source_count));
    let empty_or_failed = sources_in_project.map_or_else(
        || {
            metadata
                .skipped_documents
                .saturating_add(metadata.overview_context.failed_sources)
        },
        |sources| sources.saturating_sub(document_count),
    );
    let population_reading = match sources_in_project {
        Some(sources) if empty_or_failed == 0 && document_count == sources => format!(
            "No empty or failed sources; the analysis population contains all {sources} project sources."
        ),
        Some(sources) => format!(
            "{empty_or_failed} sources were empty or failed; analysis contains {document_count} of {sources} project sources."
        ),
        None => format!("Analysis contains {document_count} sources."),
    };

    let percent = |value: Option<f64>| {
        value
            .map(|number| format!("{:.1}%", number * 100.0))
            .unwrap_or_else(|| "unavailable".to_string())
    };
    let balance_finding_text = if document_count == 0 {
        "No documents are available for a balance assessment.".to_string()
    } else if total_tokens == 0 {
        "All document token counts are zero, so document balance is unavailable.".to_string()
    } else if document_count == 1 {
        "The analysis contains one document, so between-document balance cannot be assessed."
            .to_string()
    } else {
        format!(
            "The largest document contains {} of tokens; the top three contain {}. Effective documents: {:.1} of {}.",
            percent(largest_share),
            percent(top3_share),
            effective_documents.unwrap_or(0.0),
            document_count
        )
    };
    let extraction_finding_text = if total_types == 0 {
        "No token types are available for extraction-quality diagnostics.".to_string()
    } else {
        format!(
            "Numeric-only forms account for {} of {} types; single-character forms account for {}.",
            percent(type_share(numeric_only_types, total_types)),
            total_types,
            percent(type_share(single_character_types, total_types))
        )
    };
    let lexical_finding_text = if total_types == 0 {
        "No token types were counted.".to_string()
    } else {
        format!(
            "{} of {} types occur once, and {} occur at least 10 times.",
            hapax_count, total_types, band_10plus_count
        )
    };
    let dispersion_finding_text = if document_count < 2 {
        "Dispersion requires at least two documents and is not assessed here.".to_string()
    } else if frequent_terms.is_empty() {
        "No types occur at least 10 times, so frequent-type DP is unavailable.".to_string()
    } else {
        format!(
            "Median DP is {:.2}; {} of {} frequent types have DP at least {:.1}.",
            median_dp.unwrap_or(0.0),
            high_dp_frequent_types,
            frequent_terms.len(),
            HIGH_DISPERSION_THRESHOLD
        )
    };
    let findings = vec![
        finding(
            CorpusOverviewFindingKind::Balance,
            balance_severity,
            balance_finding_text,
        ),
        finding(
            CorpusOverviewFindingKind::ExtractionQuality,
            extraction_severity,
            extraction_finding_text,
        ),
        finding(
            CorpusOverviewFindingKind::LexicalSpectrum,
            lexical_severity,
            lexical_finding_text,
        ),
        finding(
            CorpusOverviewFindingKind::Dispersion,
            dispersion_severity,
            dispersion_finding_text,
        ),
    ];
    let highest_severity = findings
        .iter()
        .map(|finding| finding.severity)
        .max()
        .unwrap_or(CorpusOverviewFindingSeverity::NoIssue);
    let verdict = if total_tokens == 0 {
        "The analysis contains no tokens; inspect source extraction and processing before analysis."
            .to_string()
    } else {
        match highest_severity {
            CorpusOverviewFindingSeverity::NeedsFixing =>
                "The corpus has diagnostics that need fixing before aggregate results are relied on."
                    .to_string(),
            CorpusOverviewFindingSeverity::WorthALook =>
                "The corpus is available for exploration, with diagnostics worth inspecting alongside aggregate results."
                    .to_string(),
            CorpusOverviewFindingSeverity::NoIssue =>
                "No immediate issue is indicated by these first-stage corpus diagnostics."
                    .to_string(),
        }
    };

    let balance_reading = if document_count < 2 {
        "Analysis unit: the document. At least two documents are required to assess document balance."
            .to_string()
    } else if balance_severity == CorpusOverviewFindingSeverity::NoIssue {
        "Analysis unit: the document. Token totals are descriptive; read aggregate frequencies alongside dispersion even when document lengths are relatively even."
            .to_string()
    } else {
        "Analysis unit: the document. Token totals are descriptive, but a few documents carry a disproportionate share of the token mass; read aggregate frequencies alongside dispersion, not on their own."
            .to_string()
    };

    let displayed_document_count = if document_count > 50 {
        OVERVIEW_DOCUMENT_LIMIT
    } else {
        document_count
    };
    let overview_documents = sorted_documents
        .iter()
        .take(displayed_document_count)
        .map(|document| CorpusOverviewDocument {
            name: {
                let name = document_name(document);
                if name.is_empty() {
                    "Unnamed source".to_string()
                } else {
                    name.to_string()
                }
            },
            tokens: document.token_count,
            share: (total_tokens > 0).then(|| document.token_count as f64 / total_tokens as f64),
            relative_length: sorted_documents
                .first()
                .filter(|largest| largest.token_count > 0)
                .map(|largest| document.token_count as f64 / largest.token_count as f64),
        })
        .collect::<Vec<_>>();

    CorpusOverview {
        document_count,
        source_count: sources_in_project.unwrap_or(metadata.source_count),
        total_tokens,
        total_types,
        annotation_available,
        input_text_source: metadata.input_text_source,
        built_at: metadata.built_at.clone(),
        build_signature: metadata.build_signature,
        processing_config_signature: metadata.processing_config_signature,
        skipped_documents: metadata.skipped_documents,
        skipped_pdf_documents: metadata.skipped_pdf_documents,
        totals: CorpusOverviewTotals {
            tokens: total_tokens,
            types: total_types,
            documents: document_count,
        },
        population: CorpusOverviewPopulation {
            sources_in_project,
            included_in_analysis: document_count,
            processed_used: metadata.processed_documents,
            using_source_text_fallback: metadata.source_text_fallback_documents,
            empty_or_failed,
            sources_with_warnings: metadata.overview_context.sources_with_warnings,
            input_source: analysis_input_label(
                metadata.processed_documents,
                metadata.source_text_fallback_documents,
            ),
            reading: population_reading,
        },
        documents: overview_documents,
        documents_truncated: document_count > 50,
        balance: CorpusOverviewBalance {
            largest_share,
            top3_share,
            gini,
            coefficient_of_variation,
            effective_documents,
            median_tokens,
            minimum_tokens: token_counts.first().copied(),
            maximum_tokens: token_counts.last().copied(),
            remaining_documents: document_count.saturating_sub(displayed_document_count),
            reading: balance_reading,
        },
        extraction_quality: CorpusOverviewExtractionQuality {
            word_like_count,
            word_like_share: type_share(word_like_count, total_types),
            numeric_only_count: numeric_only_types,
            numeric_only_share: type_share(numeric_only_types, total_types),
            single_character_count: Some(single_character_types),
            single_character_share: type_share(single_character_types, total_types),
            reading: "Numeric-only forms may include page numbers, table fragments, formula fragments, references, or legitimate numeric examples. Inspect before removing numeric-heavy material."
                .to_string(),
        },
        lexical_spectrum: CorpusOverviewLexicalSpectrum {
            types: total_types,
            hapax_count,
            dis_legomena_count: dis_legomena,
            band_3_to_9_count,
            band_10plus_count,
            hapax_share: type_share(hapax_count, total_types),
            dis_legomena_share: type_share(dis_legomena, total_types),
            band_3_to_9_share: type_share(band_3_to_9_count, total_types),
            band_10plus_share: type_share(band_10plus_count, total_types),
            frequency_spectrum,
            frequency_spectrum_truncated,
            frequency_spectrum_total,
            mattr_window: MATTR_WINDOW_TOKENS as u32,
            mattr_median: mattr_summary.median,
            mattr_min: mattr_summary.minimum,
            mattr_max: mattr_summary.maximum,
            mattr_docs_excluded: u32::try_from(mattr_summary.documents_excluded)
                .unwrap_or(u32::MAX),
            mattr_docs_used: u32::try_from(mattr_summary.documents_used).unwrap_or(u32::MAX),
            mtld_threshold: MTLD_TTR_THRESHOLD,
            mtld_min_document_tokens: MTLD_MIN_DOCUMENT_TOKENS as u32,
            mtld_median: mtld_summary.median,
            mtld_min: mtld_summary.minimum,
            mtld_max: mtld_summary.maximum,
            mtld_docs_excluded: u32::try_from(mtld_summary.documents_excluded)
                .unwrap_or(u32::MAX),
            mtld_docs_used: u32::try_from(mtld_summary.documents_used).unwrap_or(u32::MAX),
            reading: "A long tail dominated by rare words is expected in natural-language corpora; it is not an extraction-quality problem."
                .to_string(),
        },
        frequency_concentration: CorpusOverviewFrequencyConcentration {
            top_10_coverage,
            top_100_coverage,
            top_1000_coverage,
            top_10000_coverage,
            cumulative_coverage,
            shannon_entropy_bits,
            evenness_j,
            effective_vocabulary_entropy,
            reading: "A small set of high-frequency forms can carry much of the token mass even when most types are rare."
                .to_string(),
        },
        rank_frequency,
        vocabulary_growth,
        dispersion: CorpusOverviewDispersion {
            frequent_types: frequent_terms.len(),
            high_dp_frequent_types,
            median_dp,
            df_in_1_count,
            df_in_2_to_9_count,
            df_in_10_to_99_count,
            df_in_100_plus_count,
            df_in_all_count,
            df_in_1_sample_types: df_in_1_samples.into_ordered_tokens(),
            df_in_2_to_9_sample_types: df_in_2_to_9_samples.into_ordered_tokens(),
            df_in_10_to_99_sample_types: df_in_10_to_99_samples.into_ordered_tokens(),
            df_in_100_plus_sample_types: df_in_100_plus_samples.into_ordered_tokens(),
            df_in_all_sample_types: df_in_all_samples.into_ordered_tokens(),
            df_in_1_share: type_share(df_in_1_count, total_types),
            df_in_2_to_9_share: type_share(df_in_2_to_9_count, total_types),
            df_in_10_to_99_share: type_share(df_in_10_to_99_count, total_types),
            df_in_100_plus_share: type_share(df_in_100_plus_count, total_types),
            df_in_all_share: type_share(df_in_all_count, total_types),
            df_at_least_1pct_count,
            df_at_least_1pct_sample_types: df_at_least_1pct_samples.into_ordered_tokens(),
            df_at_least_1pct_share: type_share(df_at_least_1pct_count, total_types),
            df_at_least_10pct_count,
            df_at_least_10pct_sample_types: df_at_least_10pct_samples.into_ordered_tokens(),
            df_at_least_10pct_share: type_share(df_at_least_10pct_count, total_types),
            df_at_least_50pct_count,
            df_at_least_50pct_sample_types: df_at_least_50pct_samples.into_ordered_tokens(),
            df_at_least_50pct_share: type_share(df_at_least_50pct_count, total_types),
            df_at_least_90pct_count,
            df_at_least_90pct_sample_types: df_at_least_90pct_samples.into_ordered_tokens(),
            df_at_least_90pct_share: type_share(df_at_least_90pct_count, total_types),
            universal_types,
            median_documents_per_type,
            reading: if document_count < 2 {
                "Dispersion and document-frequency spread require at least two documents."
                    .to_string()
            } else {
                "Most types may occur in few documents, which is common in sparse vocabularies. Interpret this with token-mass coverage and frequent-type dispersion."
                    .to_string()
            },
        },
        reading: CorpusOverviewReading { verdict, findings },
        build: CorpusOverviewBuild {
            tokenisation: Some(
                "UAX #29 word boundaries; ASCII Letter/Number hyphen chains and U+0027/U+2019 contractions retained; dictionary-free CJK/Thai segmentation remains limited"
                    .to_string(),
            ),
            case_handling: Some(
                "Surface case preserved; NFC Unicode lowercase aggregation keys".to_string(),
            ),
            lemmatisation: Some(
                "Surface-only build; lemma/POS annotation is not built yet.".to_string(),
            ),
            number_punctuation_handling: Some(
                "Numbers retained; decimal points and thousands separators split; standalone punctuation excluded from corpus metrics"
                    .to_string(),
            ),
            input_source: analysis_input_label(
                metadata.processed_documents,
                metadata.source_text_fallback_documents,
            ),
            config_version: metadata.overview_context.config_version,
            snapshot_date: Some(metadata.built_at),
            metadata_attached: metadata.overview_context.metadata_attached,
        },
    }
}

#[cfg(test)]
#[path = "overview_tests.rs"]
mod tests;
