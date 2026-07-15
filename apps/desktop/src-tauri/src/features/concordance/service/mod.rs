//! Routes Concordance and Collocates between scan and persisted-index backends.
//!
//! The service owns fallback policy, warm index contexts, parity audit, and timing evidence. Tauri
//! commands provide records and progress sinks; analytical execution remains in the core crate.

use corpuswright_core::analysis::{
    ConcordanceBackendUsed, ConcordanceCollocationAggregate, ConcordanceCollocationQuery,
    ConcordanceDocument, ConcordanceQuery, ConcordanceResult,
    RoutedConcordanceCollocationAggregate, RoutedConcordanceResult, search_concordance_documents,
};
use std::time::{Duration, Instant};

mod backend;
mod indexed;
mod parity;
mod recovery;
mod scan;
mod telemetry;

pub(crate) use backend::{ConcordanceBackendMode, concordance_backend_mode_from_env};
#[cfg(test)]
use backend::{concordance_backend_used_is_index_error_fallback, parse_concordance_backend_mode};
use backend::{concordance_backend_used_label, concordance_fallback_label};
pub(crate) use indexed::{
    aggregate_concordance_collocations_with_warm_index_provider,
    search_concordance_documents_with_warm_index, search_concordance_with_warm_index_provider,
};
use parity::{
    ConcordanceAuditDifference, compare_concordance_collocation_aggregates,
    compare_concordance_results, concordance_materialised_row_count,
};
use recovery::concordance_default_index_failure_state_label;
pub(crate) use recovery::{
    concordance_default_index_disabled_for_key, record_concordance_default_index_error,
};
pub(crate) use scan::{
    aggregate_concordance_collocations_scan_provider,
    search_concordance_documents_routed_scan_provider,
};
use telemetry::{duration_ms, log_string};

use super::state::{
    ConcordanceDefaultIndexFailureState, ConcordanceIndexContextHandle, ConcordanceIndexContextKey,
};

#[cfg(test)]
use super::state::ConcordanceIndexContextCache;
#[cfg(test)]
use corpuswright_core::analysis::{
    AnalysisDocumentSelection, aggregate_concordance_collocations_documents,
    query_concordance_collocations_from_aggregate,
};
#[cfg(test)]
use std::sync::{Arc, Mutex};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConcordanceIndexRecoveryReason {
    IndexError,
    PreviousIndexError,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConcordanceSearchOutcome {
    pub(crate) result: ConcordanceResult,
    pub(crate) backend_used: ConcordanceBackendUsed,
    pub(crate) index_rebuilt: Option<bool>,
    pub(crate) recovery_reason: Option<ConcordanceIndexRecoveryReason>,
}

#[cfg(test)]
impl ConcordanceSearchOutcome {
    pub(crate) fn from_routed(routed: RoutedConcordanceResult) -> Self {
        Self {
            result: routed.result,
            backend_used: routed.backend_used,
            index_rebuilt: Some(routed.index_rebuilt),
            recovery_reason: None,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConcordanceCollocationAggregationOutcome {
    pub(crate) aggregate: ConcordanceCollocationAggregate,
    pub(crate) backend_used: ConcordanceBackendUsed,
    pub(crate) index_rebuilt: Option<bool>,
    pub(crate) recovery_reason: Option<ConcordanceIndexRecoveryReason>,
}

#[cfg(test)]
impl ConcordanceCollocationAggregationOutcome {
    pub(crate) fn from_routed(routed: RoutedConcordanceCollocationAggregate) -> Self {
        Self {
            aggregate: routed.aggregate,
            backend_used: routed.backend_used,
            index_rebuilt: Some(routed.index_rebuilt),
            recovery_reason: None,
        }
    }
}

pub(crate) fn log_concordance_index_result(
    mode: ConcordanceBackendMode,
    query: &ConcordanceQuery,
    routed: &RoutedConcordanceResult,
    index_elapsed: Duration,
) {
    eprintln!(
        "[concordance-backend] mode={} backend={} fallback={} rebuilt={} elapsed_ms={} query={} case_sensitive={} hits={} rows={}",
        mode.label(),
        concordance_backend_used_label(routed.backend_used),
        concordance_fallback_label(routed.backend_used),
        routed.index_rebuilt,
        duration_ms(index_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        routed.result.total_hits,
        concordance_materialised_row_count(&routed.result)
    );
}

pub(crate) fn log_concordance_index_error(
    mode: ConcordanceBackendMode,
    query: &ConcordanceQuery,
    index_elapsed: Duration,
    error: &corpuswright_core::analysis::ConcordanceError,
) {
    eprintln!(
        "[concordance-backend] mode={} backend=index fallback=none rebuilt=unknown elapsed_ms={} status=index_error query={} case_sensitive={} error={}",
        mode.label(),
        duration_ms(index_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        log_string(&error.to_string(), 160)
    );
}

pub(crate) fn log_concordance_default_index_recovery(
    query: &ConcordanceQuery,
    scan: &ConcordanceResult,
    index_elapsed: Duration,
    scan_elapsed: Duration,
    error: &corpuswright_core::analysis::ConcordanceError,
    failure_state: ConcordanceDefaultIndexFailureState,
) {
    eprintln!(
        "[concordance-backend] mode=default_index backend=index recovery=index_error retry_state={} index_ms={} scan_recovery_ms={} query={} case_sensitive={} hits={} rows={} error={}",
        concordance_default_index_failure_state_label(failure_state),
        duration_ms(index_elapsed),
        duration_ms(scan_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.total_hits,
        concordance_materialised_row_count(scan),
        log_string(&error.to_string(), 160)
    );
}

pub(crate) fn log_concordance_default_index_recovery_scan_error(
    query: &ConcordanceQuery,
    index_elapsed: Duration,
    scan_elapsed: Duration,
    index_error: &corpuswright_core::analysis::ConcordanceError,
    scan_error: &corpuswright_core::analysis::ConcordanceError,
    failure_state: ConcordanceDefaultIndexFailureState,
) {
    eprintln!(
        "[concordance-backend] mode=default_index backend=index recovery=index_error retry_state={} index_ms={} scan_recovery_ms={} scan_status=error query={} case_sensitive={} index_error={} scan_error={}",
        concordance_default_index_failure_state_label(failure_state),
        duration_ms(index_elapsed),
        duration_ms(scan_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        log_string(&index_error.to_string(), 160),
        log_string(&scan_error.to_string(), 160)
    );
}

pub(crate) fn log_concordance_default_index_previous_error_recovery(
    query: &ConcordanceQuery,
    scan: &ConcordanceResult,
    scan_elapsed: Duration,
) {
    eprintln!(
        "[concordance-backend] mode=default_index backend=scan recovery=previous_index_error scan_recovery_ms={} query={} case_sensitive={} hits={} rows={}",
        duration_ms(scan_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.total_hits,
        concordance_materialised_row_count(scan)
    );
}

#[cfg(test)]
pub(crate) fn search_concordance_documents_strict_index_mode_with<SearchIndex>(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    search_index: SearchIndex,
) -> Result<ConcordanceSearchOutcome, String>
where
    SearchIndex:
        FnOnce(
            &[ConcordanceDocument],
            &ConcordanceQuery,
            &corpuswright_core::text::tokenisation::TokenizationConfig,
            &ConcordanceIndexContextHandle,
            ConcordanceIndexContextKey,
        )
            -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError>,
{
    let index_started_at = Instant::now();
    let routed = search_index(documents, query, tokenization_config, context_state, key);
    let index_elapsed = index_started_at.elapsed();
    match routed {
        Ok(routed) => {
            log_concordance_index_result(
                ConcordanceBackendMode::StrictIndex,
                query,
                &routed,
                index_elapsed,
            );
            Ok(ConcordanceSearchOutcome::from_routed(routed))
        }
        Err(error) => {
            log_concordance_index_error(
                ConcordanceBackendMode::StrictIndex,
                query,
                index_elapsed,
                &error,
            );
            Err(error.to_string())
        }
    }
}

#[cfg(test)]
pub(crate) fn search_concordance_documents_default_index_mode(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
) -> Result<ConcordanceSearchOutcome, String> {
    search_concordance_documents_default_index_mode_with(
        documents,
        query,
        tokenization_config,
        context_state,
        key,
        search_concordance_documents,
        search_concordance_documents_with_warm_index,
    )
}

#[cfg(test)]
pub(crate) fn search_concordance_documents_default_index_mode_with<SearchScan, SearchIndex>(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    search_scan: SearchScan,
    search_index: SearchIndex,
) -> Result<ConcordanceSearchOutcome, String>
where
    SearchScan: Fn(
        &[ConcordanceDocument],
        &ConcordanceQuery,
        &corpuswright_core::text::tokenisation::TokenizationConfig,
    ) -> Result<ConcordanceResult, corpuswright_core::analysis::ConcordanceError>,
    SearchIndex:
        FnOnce(
            &[ConcordanceDocument],
            &ConcordanceQuery,
            &corpuswright_core::text::tokenisation::TokenizationConfig,
            &ConcordanceIndexContextHandle,
            ConcordanceIndexContextKey,
        )
            -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError>,
{
    if concordance_default_index_disabled_for_key(context_state, &key) {
        let scan_started_at = Instant::now();
        let scan = search_scan(documents, query, tokenization_config)
            .map_err(|error| error.to_string())?;
        let scan_elapsed = scan_started_at.elapsed();
        log_concordance_default_index_previous_error_recovery(query, &scan, scan_elapsed);
        return Ok(ConcordanceSearchOutcome {
            result: scan,
            backend_used: ConcordanceBackendUsed::Scan,
            index_rebuilt: None,
            recovery_reason: Some(ConcordanceIndexRecoveryReason::PreviousIndexError),
        });
    }

    let index_started_at = Instant::now();
    let routed = search_index(
        documents,
        query,
        tokenization_config,
        context_state,
        key.clone(),
    );
    let index_elapsed = index_started_at.elapsed();
    match routed {
        Ok(routed) => {
            log_concordance_index_result(
                ConcordanceBackendMode::DefaultIndex,
                query,
                &routed,
                index_elapsed,
            );
            Ok(ConcordanceSearchOutcome::from_routed(routed))
        }
        Err(index_error) => {
            if index_error.prevents_unbounded_scan_fallback() {
                log_concordance_index_error(
                    ConcordanceBackendMode::DefaultIndex,
                    query,
                    index_elapsed,
                    &index_error,
                );
                return Err(index_error.to_string());
            }
            let failure_state = record_concordance_default_index_error(context_state, &key);
            let scan_started_at = Instant::now();
            let scan = search_scan(documents, query, tokenization_config);
            let scan_elapsed = scan_started_at.elapsed();
            match scan {
                Ok(scan) => {
                    log_concordance_default_index_recovery(
                        query,
                        &scan,
                        index_elapsed,
                        scan_elapsed,
                        &index_error,
                        failure_state,
                    );
                    Ok(ConcordanceSearchOutcome {
                        result: scan,
                        backend_used: ConcordanceBackendUsed::Scan,
                        index_rebuilt: None,
                        recovery_reason: Some(ConcordanceIndexRecoveryReason::IndexError),
                    })
                }
                Err(scan_error) => {
                    log_concordance_default_index_recovery_scan_error(
                        query,
                        index_elapsed,
                        scan_elapsed,
                        &index_error,
                        &scan_error,
                        failure_state,
                    );
                    Err(scan_error.to_string())
                }
            }
        }
    }
}

pub(crate) fn log_concordance_audit_result(
    query: &ConcordanceQuery,
    scan: &ConcordanceResult,
    routed: &RoutedConcordanceResult,
    scan_elapsed: Duration,
    index_elapsed: Duration,
    difference: Option<&ConcordanceAuditDifference>,
) {
    let parity = match routed.backend_used {
        ConcordanceBackendUsed::ScanFallback { .. } => "scan_fallback",
        ConcordanceBackendUsed::Scan | ConcordanceBackendUsed::Index => {
            if difference.is_some() {
                "mismatch"
            } else {
                "match"
            }
        }
    };
    let difference_summary = difference
        .map(|difference| {
            format!(
                " diff_field={} diff_row={} scan={} index={}",
                difference.field,
                difference
                    .row_index
                    .map(|index| index.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                log_string(&difference.scan, 120),
                log_string(&difference.index, 120)
            )
        })
        .unwrap_or_default();
    eprintln!(
        "[concordance-backend] mode=audit backend={} fallback={} rebuilt={} scan_ms={} index_ms={} parity={} query={} case_sensitive={} hits={} rows={}{}",
        concordance_backend_used_label(routed.backend_used),
        concordance_fallback_label(routed.backend_used),
        routed.index_rebuilt,
        duration_ms(scan_elapsed),
        duration_ms(index_elapsed),
        parity,
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.total_hits,
        concordance_materialised_row_count(scan),
        difference_summary
    );
}

pub(crate) fn log_concordance_audit_index_error(
    query: &ConcordanceQuery,
    scan: &ConcordanceResult,
    scan_elapsed: Duration,
    index_elapsed: Duration,
    error: &corpuswright_core::analysis::ConcordanceError,
) {
    eprintln!(
        "[concordance-backend] mode=audit backend=index fallback=none rebuilt=unknown scan_ms={} index_ms={} parity=index_error query={} case_sensitive={} hits={} rows={} error={}",
        duration_ms(scan_elapsed),
        duration_ms(index_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.total_hits,
        concordance_materialised_row_count(scan),
        log_string(&error.to_string(), 160)
    );
}

pub(crate) fn search_concordance_documents_audit_mode(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
) -> Result<ConcordanceResult, String> {
    let scan_started_at = Instant::now();
    let scan = search_concordance_documents(documents, query, tokenization_config)
        .map_err(|error| error.to_string())?;
    let scan_elapsed = scan_started_at.elapsed();

    let index_started_at = Instant::now();
    let routed = search_concordance_documents_with_warm_index(
        documents,
        query,
        tokenization_config,
        context_state,
        key,
    );
    let index_elapsed = index_started_at.elapsed();
    match routed {
        Ok(routed) => {
            let difference = match routed.backend_used {
                ConcordanceBackendUsed::ScanFallback { .. } => None,
                ConcordanceBackendUsed::Scan | ConcordanceBackendUsed::Index => {
                    compare_concordance_results(&scan, &routed.result)
                }
            };
            log_concordance_audit_result(
                query,
                &scan,
                &routed,
                scan_elapsed,
                index_elapsed,
                difference.as_ref(),
            );
        }
        Err(error) => {
            log_concordance_audit_index_error(query, &scan, scan_elapsed, index_elapsed, &error);
        }
    }

    Ok(scan)
}

pub(crate) fn log_concordance_collocation_index_result(
    mode: ConcordanceBackendMode,
    query: &ConcordanceCollocationQuery,
    routed: &RoutedConcordanceCollocationAggregate,
    index_elapsed: Duration,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode={} backend={} fallback={} rebuilt={} elapsed_ms={} query={} case_sensitive={} f_node={} candidates={} n={}",
        mode.label(),
        concordance_backend_used_label(routed.backend_used),
        concordance_fallback_label(routed.backend_used),
        routed.index_rebuilt,
        duration_ms(index_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        routed.aggregate.f_node,
        routed.aggregate.candidate_counts.len(),
        routed.aggregate.n
    );
}

pub(crate) fn log_concordance_collocation_index_error(
    mode: ConcordanceBackendMode,
    query: &ConcordanceCollocationQuery,
    index_elapsed: Duration,
    error: &corpuswright_core::analysis::ConcordanceError,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode={} backend=index fallback=none rebuilt=unknown elapsed_ms={} status=index_error query={} case_sensitive={} error={}",
        mode.label(),
        duration_ms(index_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        log_string(&error.to_string(), 160)
    );
}

#[cfg(test)]
pub(crate) fn log_concordance_collocation_default_index_recovery(
    query: &ConcordanceCollocationQuery,
    scan: &ConcordanceCollocationAggregate,
    index_elapsed: Duration,
    scan_elapsed: Duration,
    error: &corpuswright_core::analysis::ConcordanceError,
    failure_state: ConcordanceDefaultIndexFailureState,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode=default_index backend=index recovery=index_error retry_state={} index_ms={} scan_recovery_ms={} query={} case_sensitive={} f_node={} candidates={} n={} error={}",
        concordance_default_index_failure_state_label(failure_state),
        duration_ms(index_elapsed),
        duration_ms(scan_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.f_node,
        scan.candidate_counts.len(),
        scan.n,
        log_string(&error.to_string(), 160)
    );
}

#[cfg(test)]
pub(crate) fn log_concordance_collocation_default_index_recovery_scan_error(
    query: &ConcordanceCollocationQuery,
    index_elapsed: Duration,
    scan_elapsed: Duration,
    index_error: &corpuswright_core::analysis::ConcordanceError,
    scan_error: &corpuswright_core::analysis::ConcordanceError,
    failure_state: ConcordanceDefaultIndexFailureState,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode=default_index backend=index recovery=index_error retry_state={} index_ms={} scan_recovery_ms={} scan_status=error query={} case_sensitive={} index_error={} scan_error={}",
        concordance_default_index_failure_state_label(failure_state),
        duration_ms(index_elapsed),
        duration_ms(scan_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        log_string(&index_error.to_string(), 160),
        log_string(&scan_error.to_string(), 160)
    );
}

#[cfg(test)]
pub(crate) fn log_concordance_collocation_default_index_previous_error_recovery(
    query: &ConcordanceCollocationQuery,
    scan: &ConcordanceCollocationAggregate,
    scan_elapsed: Duration,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode=default_index backend=scan recovery=previous_index_error scan_recovery_ms={} query={} case_sensitive={} f_node={} candidates={} n={}",
        duration_ms(scan_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.f_node,
        scan.candidate_counts.len(),
        scan.n
    );
}

#[cfg(test)]
pub(crate) fn log_concordance_collocation_default_index_fallback_failure(
    query: &ConcordanceCollocationQuery,
    routed: &RoutedConcordanceCollocationAggregate,
    failure_state: ConcordanceDefaultIndexFailureState,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode=default_index backend={} fallback={} recovery=index_error retry_state={} query={} case_sensitive={} f_node={} candidates={} n={}",
        concordance_backend_used_label(routed.backend_used),
        concordance_fallback_label(routed.backend_used),
        concordance_default_index_failure_state_label(failure_state),
        log_string(&query.query, 80),
        query.case_sensitive,
        routed.aggregate.f_node,
        routed.aggregate.candidate_counts.len(),
        routed.aggregate.n
    );
}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn aggregate_concordance_collocations_default_index_mode<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken,
    resolve: Resolve,
    report_progress: Progress,
) -> Result<ConcordanceCollocationAggregationOutcome, String>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument> + Clone,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress) + Clone,
{
    aggregate_concordance_collocations_default_index_mode_with(
        document_slots,
        query,
        tokenization_config,
        context_state,
        key,
        cancellation,
        resolve,
        report_progress,
        |slots, q, t, cancel, res, _| {
            aggregate_concordance_collocations_scan_provider(slots, q, t, cancel, res)
                .map(|r| r.aggregate)
        },
        aggregate_concordance_collocations_with_warm_index_provider,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn aggregate_concordance_collocations_default_index_mode_with<
    SearchScan,
    SearchIndex,
    Resolve,
    Progress,
>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken,
    resolve: Resolve,
    report_progress: Progress,
    search_scan: SearchScan,
    search_index: SearchIndex,
) -> Result<ConcordanceCollocationAggregationOutcome, String>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument> + Clone,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress) + Clone,
    SearchScan: Fn(
        usize,
        &ConcordanceCollocationQuery,
        &corpuswright_core::text::tokenisation::TokenizationConfig,
        &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken,
        Resolve,
        Progress,
    ) -> Result<
        ConcordanceCollocationAggregate,
        corpuswright_core::analysis::ConcordanceError,
    >,
    SearchIndex: FnOnce(
        usize,
        &ConcordanceCollocationQuery,
        &corpuswright_core::text::tokenisation::TokenizationConfig,
        &ConcordanceIndexContextHandle,
        ConcordanceIndexContextKey,
        &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken,
        Resolve,
        Progress,
    ) -> Result<
        RoutedConcordanceCollocationAggregate,
        corpuswright_core::analysis::ConcordanceError,
    >,
{
    if concordance_default_index_disabled_for_key(context_state, &key) {
        let scan_started_at = Instant::now();
        let scan = search_scan(
            document_slots,
            query,
            tokenization_config,
            cancellation,
            resolve.clone(),
            report_progress.clone(),
        )
        .map_err(|error| error.to_string())?;
        let scan_elapsed = scan_started_at.elapsed();
        log_concordance_collocation_default_index_previous_error_recovery(
            query,
            &scan,
            scan_elapsed,
        );
        return Ok(ConcordanceCollocationAggregationOutcome {
            aggregate: scan,
            backend_used: ConcordanceBackendUsed::Scan,
            index_rebuilt: None,
            recovery_reason: Some(ConcordanceIndexRecoveryReason::PreviousIndexError),
        });
    }

    let index_started_at = Instant::now();
    let routed = search_index(
        document_slots,
        query,
        tokenization_config,
        context_state,
        key.clone(),
        cancellation,
        resolve.clone(),
        report_progress.clone(),
    );
    let index_elapsed = index_started_at.elapsed();
    match routed {
        Ok(routed) => {
            log_concordance_collocation_index_result(
                ConcordanceBackendMode::DefaultIndex,
                query,
                &routed,
                index_elapsed,
            );
            if concordance_backend_used_is_index_error_fallback(routed.backend_used) {
                let failure_state = record_concordance_default_index_error(context_state, &key);
                log_concordance_collocation_default_index_fallback_failure(
                    query,
                    &routed,
                    failure_state,
                );
                return Ok(ConcordanceCollocationAggregationOutcome {
                    aggregate: routed.aggregate,
                    backend_used: routed.backend_used,
                    index_rebuilt: Some(routed.index_rebuilt),
                    recovery_reason: Some(ConcordanceIndexRecoveryReason::IndexError),
                });
            }
            Ok(ConcordanceCollocationAggregationOutcome::from_routed(
                routed,
            ))
        }
        Err(index_error) => {
            if index_error.prevents_unbounded_scan_fallback() {
                log_concordance_collocation_index_error(
                    ConcordanceBackendMode::DefaultIndex,
                    query,
                    index_elapsed,
                    &index_error,
                );
                return Err(index_error.to_string());
            }
            let failure_state = record_concordance_default_index_error(context_state, &key);
            let scan_started_at = Instant::now();
            let scan = search_scan(
                document_slots,
                query,
                tokenization_config,
                cancellation,
                resolve.clone(),
                report_progress.clone(),
            );
            let scan_elapsed = scan_started_at.elapsed();
            match scan {
                Ok(scan) => {
                    log_concordance_collocation_default_index_recovery(
                        query,
                        &scan,
                        index_elapsed,
                        scan_elapsed,
                        &index_error,
                        failure_state,
                    );
                    Ok(ConcordanceCollocationAggregationOutcome {
                        aggregate: scan,
                        backend_used: ConcordanceBackendUsed::Scan,
                        index_rebuilt: None,
                        recovery_reason: Some(ConcordanceIndexRecoveryReason::IndexError),
                    })
                }
                Err(scan_error) => {
                    log_concordance_collocation_default_index_recovery_scan_error(
                        query,
                        index_elapsed,
                        scan_elapsed,
                        &index_error,
                        &scan_error,
                        failure_state,
                    );
                    Err(scan_error.to_string())
                }
            }
        }
    }
}

pub(crate) fn log_concordance_collocation_audit_result(
    query: &ConcordanceCollocationQuery,
    scan: &ConcordanceCollocationAggregate,
    routed: &RoutedConcordanceCollocationAggregate,
    scan_elapsed: Duration,
    index_elapsed: Duration,
    difference: Option<&ConcordanceAuditDifference>,
) {
    let parity = match routed.backend_used {
        ConcordanceBackendUsed::ScanFallback { .. } => "scan_fallback",
        ConcordanceBackendUsed::Scan | ConcordanceBackendUsed::Index => {
            if difference.is_some() {
                "mismatch"
            } else {
                "match"
            }
        }
    };
    let difference_summary = difference
        .map(|difference| {
            format!(
                " diff_field={} scan={} index={}",
                difference.field,
                log_string(&difference.scan, 120),
                log_string(&difference.index, 120)
            )
        })
        .unwrap_or_default();
    eprintln!(
        "[concordance-backend] feature=collocation mode=audit backend={} fallback={} rebuilt={} scan_ms={} index_ms={} parity={} query={} case_sensitive={} f_node={} candidates={} n={}{}",
        concordance_backend_used_label(routed.backend_used),
        concordance_fallback_label(routed.backend_used),
        routed.index_rebuilt,
        duration_ms(scan_elapsed),
        duration_ms(index_elapsed),
        parity,
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.f_node,
        scan.candidate_counts.len(),
        scan.n,
        difference_summary
    );
}

pub(crate) fn log_concordance_collocation_audit_index_error(
    query: &ConcordanceCollocationQuery,
    scan: &ConcordanceCollocationAggregate,
    scan_elapsed: Duration,
    index_elapsed: Duration,
    error: &corpuswright_core::analysis::ConcordanceError,
) {
    eprintln!(
        "[concordance-backend] feature=collocation mode=audit backend=index fallback=none rebuilt=unknown scan_ms={} index_ms={} parity=index_error query={} case_sensitive={} f_node={} candidates={} n={} error={}",
        duration_ms(scan_elapsed),
        duration_ms(index_elapsed),
        log_string(&query.query, 80),
        query.case_sensitive,
        scan.f_node,
        scan.candidate_counts.len(),
        scan.n,
        log_string(&error.to_string(), 160)
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn aggregate_concordance_collocations_audit_mode<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken,
    resolve: Resolve,
    report_progress: Progress,
) -> Result<ConcordanceCollocationAggregate, String>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument> + Clone,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress) + Clone,
{
    let scan_started_at = Instant::now();
    let scan_result = aggregate_concordance_collocations_scan_provider(
        document_slots,
        query,
        tokenization_config,
        cancellation,
        resolve.clone(),
    )
    .map_err(|error| error.to_string())?;
    let scan = scan_result.aggregate;
    let scan_elapsed = scan_started_at.elapsed();

    let index_started_at = Instant::now();
    let routed = aggregate_concordance_collocations_with_warm_index_provider(
        document_slots,
        query,
        tokenization_config,
        context_state,
        key,
        cancellation,
        resolve,
        report_progress,
    );
    let index_elapsed = index_started_at.elapsed();
    match routed {
        Ok(routed) => {
            let difference = match routed.backend_used {
                ConcordanceBackendUsed::ScanFallback { .. } => None,
                ConcordanceBackendUsed::Scan | ConcordanceBackendUsed::Index => {
                    compare_concordance_collocation_aggregates(&scan, &routed.aggregate)
                }
            };
            log_concordance_collocation_audit_result(
                query,
                &scan,
                &routed,
                scan_elapsed,
                index_elapsed,
                difference.as_ref(),
            );
        }
        Err(error) => {
            log_concordance_collocation_audit_index_error(
                query,
                &scan,
                scan_elapsed,
                index_elapsed,
                &error,
            );
        }
    }

    Ok(scan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop_collocation_query(
        query: &str,
        case_sensitive: bool,
        span: usize,
        direction: corpuswright_core::analysis::ConcordanceCollocationDirection,
    ) -> ConcordanceCollocationQuery {
        ConcordanceCollocationQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            case_sensitive,
            span,
            direction,
            min_cooccurrence: 1,
            min_collocate_frequency: 1,
            hide_stopwords: false,
            language: None,
            sort_measure: corpuswright_core::analysis::ConcordanceCollocationMeasure::LogDice,
            sort_key: corpuswright_core::analysis::ConcordanceCollocationSortKey::Association,
            sort_direction: corpuswright_core::analysis::SortDirection::Desc,
            limit: 100,
        }
    }

    fn concordance_test_documents() -> Vec<ConcordanceDocument> {
        vec![
            ConcordanceDocument {
                document_index: 0,
                source_path: "a.txt".to_string(),
                display_name: "a.txt".to_string(),
                text: "Movie, movie. Good movie! I love it. ação coração não João.".to_string(),
            },
            ConcordanceDocument {
                document_index: 1,
                source_path: "b.txt".to_string(),
                display_name: "b.txt".to_string(),
                text: "A good movie is still a movie. Eu digo ação, coração, não, João."
                    .to_string(),
            },
        ]
    }

    fn concordance_test_query(query: &str) -> ConcordanceQuery {
        ConcordanceQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            context_filter: None,
            context_filter_direction:
                corpuswright_core::analysis::ConcordanceContextFilterDirection::Both,
            case_sensitive: false,
            sort_keys: vec![corpuswright_core::analysis::ConcordanceSortKey::FilePosition],
            window_tokens: 5,
            offset: 0,
            limit: 100,
            include_rows: true,
            distribution_offset: 0,
            distribution_limit: 10,
            include_distribution: true,
        }
    }

    fn concordance_test_context_key(corpus_version: u64) -> ConcordanceIndexContextKey {
        ConcordanceIndexContextKey {
            index: crate::runtime::identity::IndexIdentity::from_ipc(corpus_version, "cleaning-v1"),
        }
    }

    fn concordance_test_context() -> ConcordanceIndexContextHandle {
        Arc::new(Mutex::new(ConcordanceIndexContextCache::default()))
    }

    fn concordance_test_index_error() -> corpuswright_core::analysis::ConcordanceError {
        corpuswright_core::analysis::ConcordanceError::Index(
            corpuswright_core::analysis::concordance::index::SurfaceIndexError::DuplicateDocumentId(
                0,
            ),
        )
    }

    #[test]
    fn concordance_backend_mode_parser_covers_env_contract() {
        let cases = [
            (None, ConcordanceBackendMode::DefaultIndex, None),
            (Some(""), ConcordanceBackendMode::DefaultIndex, None),
            (Some("   "), ConcordanceBackendMode::DefaultIndex, None),
            (Some("scan"), ConcordanceBackendMode::Scan, None),
            (Some("ScAn"), ConcordanceBackendMode::Scan, None),
            (Some("index"), ConcordanceBackendMode::StrictIndex, None),
            (Some(" INDEX "), ConcordanceBackendMode::StrictIndex, None),
            (Some("Audit"), ConcordanceBackendMode::Audit, None),
            (
                Some("something-else"),
                ConcordanceBackendMode::DefaultIndex,
                Some("something-else"),
            ),
        ];

        for (value, expected_mode, expected_unknown) in cases {
            let parsed = parse_concordance_backend_mode(value);
            assert_eq!(parsed.mode, expected_mode);
            assert_eq!(parsed.unknown_value.as_deref(), expected_unknown);
        }
    }

    #[test]
    fn concordance_default_index_uses_index_for_supported_query() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        let outcome = search_concordance_documents_default_index_mode(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();

        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(outcome.recovery_reason, None);
        assert_eq!(outcome.index_rebuilt, Some(true));
        assert_eq!(outcome.result, scan);
    }

    #[test]
    fn concordance_default_index_success_does_not_invoke_scan_route() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let expected = search_concordance_documents(&documents, &query, &tokenization_config)
            .expect("fixture scan should succeed");
        let scan_calls = std::cell::Cell::new(0usize);
        let index_calls = std::cell::Cell::new(0usize);

        let outcome = search_concordance_documents_default_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
            |_, _, _| {
                scan_calls.set(scan_calls.get() + 1);
                panic!("default index success must not invoke scan")
            },
            |_, _, _, _, _| {
                index_calls.set(index_calls.get() + 1);
                Ok(RoutedConcordanceResult {
                    result: expected.clone(),
                    backend_used: ConcordanceBackendUsed::Index,
                    index_rebuilt: false,
                    execution_stats:
                        corpuswright_core::analysis::ConcordanceExecutionStats::default(),
                })
            },
        )
        .unwrap();

        assert_eq!(index_calls.get(), 1);
        assert_eq!(scan_calls.get(), 0);
        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(outcome.result, expected);
    }

    #[test]
    fn concordance_default_index_handles_context_filter() {
        let documents = concordance_test_documents();
        let mut query = concordance_test_query("movie");
        query.context_filter = Some("good".to_string());
        query.context_filter_direction =
            corpuswright_core::analysis::ConcordanceContextFilterDirection::Right;
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        let outcome = search_concordance_documents_default_index_mode(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();

        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(outcome.recovery_reason, None);
        assert_eq!(outcome.index_rebuilt, Some(true));
        assert_eq!(outcome.result, scan);
    }

    #[test]
    fn concordance_default_index_recovers_from_index_error_and_retries_clean_rebuild() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let key = concordance_test_context_key(7);
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        let primed = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key.clone(),
        )
        .unwrap();
        assert!(primed.index_rebuilt);
        assert!(context.lock().unwrap().entry.is_some());

        let index_error = concordance_test_index_error();
        let outcome = search_concordance_documents_default_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key.clone(),
            search_concordance_documents,
            |_, _, _, _, _| Err(index_error.clone()),
        )
        .unwrap();

        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Scan);
        assert_eq!(
            outcome.recovery_reason,
            Some(ConcordanceIndexRecoveryReason::IndexError)
        );
        assert_eq!(outcome.index_rebuilt, None);
        assert_eq!(outcome.result, scan);
        {
            let cache = context.lock().unwrap();
            assert!(cache.entry.is_none());
            assert_eq!(
                cache
                    .default_index_failure
                    .as_ref()
                    .map(|failure| &failure.state),
                Some(&ConcordanceDefaultIndexFailureState::RetryPending)
            );
        }

        let retry = search_concordance_documents_default_index_mode(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key,
        )
        .unwrap();

        assert_eq!(retry.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(retry.index_rebuilt, Some(true));
        assert_eq!(retry.recovery_reason, None);
        assert!(context.lock().unwrap().default_index_failure.is_none());
    }

    #[test]
    fn concordance_default_index_disables_after_repeated_index_error() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let key = concordance_test_context_key(7);
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        for expected_state in [
            ConcordanceDefaultIndexFailureState::RetryPending,
            ConcordanceDefaultIndexFailureState::Disabled,
        ] {
            let index_error = concordance_test_index_error();
            let outcome = search_concordance_documents_default_index_mode_with(
                &documents,
                &query,
                &tokenization_config,
                &context,
                key.clone(),
                search_concordance_documents,
                |_, _, _, _, _| Err(index_error.clone()),
            )
            .unwrap();
            assert_eq!(outcome.result, scan);
            assert_eq!(
                outcome.recovery_reason,
                Some(ConcordanceIndexRecoveryReason::IndexError)
            );
            assert_eq!(
                context
                    .lock()
                    .unwrap()
                    .default_index_failure
                    .as_ref()
                    .map(|failure| &failure.state),
                Some(&expected_state)
            );
        }

        let index_calls = std::cell::Cell::new(0usize);
        let outcome = search_concordance_documents_default_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key,
            search_concordance_documents,
            |_, _, _, _, _| {
                index_calls.set(index_calls.get() + 1);
                Err(concordance_test_index_error())
            },
        )
        .unwrap();

        assert_eq!(index_calls.get(), 0);
        assert_eq!(outcome.result, scan);
        assert_eq!(
            outcome.recovery_reason,
            Some(ConcordanceIndexRecoveryReason::PreviousIndexError)
        );
    }

    #[test]
    fn concordance_strict_index_surfaces_injected_index_error() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let index_error = concordance_test_index_error();

        let error = search_concordance_documents_strict_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
            |_, _, _, _, _| Err(index_error.clone()),
        )
        .unwrap_err();

        assert_eq!(error, index_error.to_string());
    }

    #[test]
    fn concordance_audit_difference_reports_first_row_identity_mismatch() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        let mut index = scan.clone();
        index.rows.as_mut().expect("rows requested")[0].token_index += 1;

        let difference = compare_concordance_results(&scan, &index).unwrap();

        assert_eq!(difference.field, "row_identity");
        assert_eq!(difference.row_index, Some(0));
        assert_ne!(difference.scan, difference.index);
    }

    #[test]
    fn concordance_audit_compares_omitted_result_sections() {
        let documents = concordance_test_documents();
        let mut query = concordance_test_query("movie");
        query.include_rows = false;
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        let mut index = scan.clone();

        assert_eq!(compare_concordance_results(&scan, &index), None);
        index.rows = Some(Vec::new());
        assert_eq!(
            compare_concordance_results(&scan, &index)
                .expect("section mismatch")
                .field,
            "rows_included"
        );

        query.include_rows = true;
        query.include_distribution = false;
        let rows_only =
            search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        assert_eq!(compare_concordance_results(&rows_only, &rows_only), None);
    }

    #[test]
    fn concordance_collocation_default_index_reuses_concordance_warm_context() {
        let documents = concordance_test_documents();
        let concordance_query = concordance_test_query("movie");
        let collocation_query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let key = concordance_test_context_key(7);
        let scan = aggregate_concordance_collocations_documents(
            &documents,
            &collocation_query,
            &tokenization_config,
            corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken::default(
            ),
        )
        .unwrap();

        let concordance = search_concordance_with_warm_index_provider(
            documents.len(),
            &concordance_query,
            &tokenization_config,
            &context,
            key.clone(),
            |document_index| Some(documents[document_index].clone()),
            |_| {},
        )
        .unwrap();
        let collocation = aggregate_concordance_collocations_default_index_mode(
            documents.len(),
            &collocation_query,
            &tokenization_config,
            &context,
            key,
            &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken::default(),
            |document_index| Some(documents[document_index].clone()),
            |_| {},
        )
        .unwrap();

        assert!(concordance.index_rebuilt);
        assert_eq!(collocation.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(collocation.index_rebuilt, Some(false));
        assert_eq!(collocation.recovery_reason, None);
        assert_eq!(
            compare_concordance_collocation_aggregates(&scan, &collocation.aggregate),
            None
        );
        assert_eq!(
            query_concordance_collocations_from_aggregate(
                &collocation.aggregate,
                &collocation_query
            ),
            query_concordance_collocations_from_aggregate(&scan, &collocation_query)
        );
    }

    #[test]
    fn concordance_collocation_memory_refusal_never_invokes_scan_fallback() {
        let documents = concordance_test_documents();
        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan_calls = std::cell::Cell::new(0usize);
        let index_error =
            corpuswright_core::analysis::ConcordanceError::AnalysisMemoryLimitExceeded {
                estimated_resident_index_bytes: 900,
                projected_aggregate_bytes: 200,
                combined_limit_bytes: 1_000,
                safety_margin_bytes: 100,
            };

        let error = aggregate_concordance_collocations_default_index_mode_with(
            documents.len(),
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
            &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken::default(),
            |document_index| Some(documents[document_index].clone()),
            |_| {},
            |_, _, _, _, _, _| {
                scan_calls.set(scan_calls.get() + 1);
                Err(concordance_test_index_error())
            },
            |_, _, _, _, _, _, _, _| Err(index_error.clone()),
        )
        .unwrap_err();

        assert_eq!(scan_calls.get(), 0);
        assert!(error.contains("Narrow the file scope"));
        assert!(context.lock().unwrap().default_index_failure.is_none());
    }

    #[test]
    fn concordance_collocation_audit_difference_reports_full_aggregate_mismatch() {
        let documents = concordance_test_documents();
        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let scan = aggregate_concordance_collocations_documents(
            &documents,
            &query,
            &tokenization_config,
            corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken::default(
            ),
        )
        .unwrap();
        let mut index = scan.clone();
        index.f_node += 1;

        let difference = compare_concordance_collocation_aggregates(&scan, &index).unwrap();

        assert_eq!(difference.field, "collocation_f_node");
        assert_ne!(difference.scan, difference.index);
    }

    #[test]
    fn concordance_warm_index_context_reuses_store_for_fixture_sequence() {
        let documents = concordance_test_documents();
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let queries = [
            "movie",
            "good movie",
            "I love it",
            "ação",
            "coração",
            "não",
            "João",
        ];
        let mut rebuild_count = 0;

        for pass in 0..2 {
            for query_text in queries.iter().copied() {
                let query = concordance_test_query(query_text);
                let scan =
                    search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
                let started_at = Instant::now();
                let routed = search_concordance_documents_with_warm_index(
                    &documents,
                    &query,
                    &tokenization_config,
                    &context,
                    concordance_test_context_key(7),
                )
                .unwrap();
                eprintln!(
                    "[concordance-backend-test] pass={} query={} index_ms={} rebuilt={}",
                    pass,
                    log_string(query_text, 80),
                    duration_ms(started_at.elapsed()),
                    routed.index_rebuilt
                );

                assert_eq!(routed.backend_used, ConcordanceBackendUsed::Index);
                assert_eq!(routed.result, scan);
                if routed.index_rebuilt {
                    rebuild_count += 1;
                }
                if pass == 1 {
                    assert!(!routed.index_rebuilt);
                }
            }
        }

        assert_eq!(rebuild_count, 1);
    }

    #[test]
    fn concordance_warm_index_context_rebuilds_when_corpus_key_changes() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();

        let first = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();
        let second = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();
        let third = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(8),
        )
        .unwrap();

        assert!(first.index_rebuilt);
        assert!(!second.index_rebuilt);
        assert!(third.index_rebuilt);
    }

    #[test]
    fn concordance_index_context_filter_uses_index() {
        let documents = concordance_test_documents();
        let mut query = concordance_test_query("movie");
        query.context_filter = Some("good".to_string());
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        let routed = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();

        assert_eq!(routed.backend_used, ConcordanceBackendUsed::Index);
        assert!(routed.index_rebuilt);
        assert_eq!(routed.result, scan);
    }

    #[test]
    fn concordance_index_errors_surface_from_warm_route() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("   ");
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let context = concordance_test_context();

        let error = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap_err();

        assert_eq!(
            error,
            corpuswright_core::analysis::ConcordanceError::EmptyQuery
        );
    }
}
