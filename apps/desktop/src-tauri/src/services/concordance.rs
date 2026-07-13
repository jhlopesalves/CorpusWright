use crate::*;

pub(crate) const CONCORDANCE_BACKEND_ENV: &str = "CORPUSWRIGHT_CONCORDANCE_BACKEND";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConcordanceBackendMode {
    DefaultIndex,
    Scan,
    StrictIndex,
    Audit,
}

impl ConcordanceBackendMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DefaultIndex => "default_index",
            Self::Scan => "scan",
            Self::StrictIndex => "strict_index",
            Self::Audit => "audit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceBackendModeParse {
    pub(crate) mode: ConcordanceBackendMode,
    pub(crate) unknown_value: Option<String>,
}

pub(crate) fn parse_concordance_backend_mode(value: Option<&str>) -> ConcordanceBackendModeParse {
    let Some(trimmed) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return ConcordanceBackendModeParse {
            mode: ConcordanceBackendMode::DefaultIndex,
            unknown_value: None,
        };
    };

    let mode = match trimmed.to_ascii_lowercase().as_str() {
        "scan" => ConcordanceBackendMode::Scan,
        "index" => ConcordanceBackendMode::StrictIndex,
        "audit" => ConcordanceBackendMode::Audit,
        _ => {
            return ConcordanceBackendModeParse {
                mode: ConcordanceBackendMode::DefaultIndex,
                unknown_value: Some(trimmed.to_string()),
            };
        }
    };

    ConcordanceBackendModeParse {
        mode,
        unknown_value: None,
    }
}

pub(crate) fn concordance_backend_mode_from_env() -> ConcordanceBackendMode {
    match std::env::var(CONCORDANCE_BACKEND_ENV) {
        Ok(value) => {
            let parsed = parse_concordance_backend_mode(Some(&value));
            if let Some(unknown_value) = parsed.unknown_value.as_deref() {
                eprintln!(
                    "[concordance-backend] mode=default_index warning=unknown_env env={} value={}",
                    CONCORDANCE_BACKEND_ENV,
                    log_string(unknown_value, 80)
                );
            }
            parsed.mode
        }
        Err(std::env::VarError::NotPresent) => ConcordanceBackendMode::DefaultIndex,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!(
                "[concordance-backend] mode=default_index warning=invalid_env_unicode env={}",
                CONCORDANCE_BACKEND_ENV
            );
            ConcordanceBackendMode::DefaultIndex
        }
    }
}

pub(crate) fn log_string(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut truncated = String::new();
    for (index, character) in compact.chars().enumerate() {
        if index >= max_chars {
            truncated.push_str("...");
            break;
        }
        truncated.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    if truncated.is_empty() {
        truncated.push_str("<empty>");
    }
    serde_json::to_string(&truncated).unwrap_or_else(|_| "\"<unavailable>\"".to_string())
}

pub(crate) fn concordance_backend_used_label(backend: ConcordanceBackendUsed) -> &'static str {
    match backend {
        ConcordanceBackendUsed::Scan => "scan",
        ConcordanceBackendUsed::Index => "index",
        ConcordanceBackendUsed::ScanFallback { .. } => "scan_fallback",
    }
}

pub(crate) fn concordance_fallback_reason_label(reason: ConcordanceFallbackReason) -> &'static str {
    match reason {
        ConcordanceFallbackReason::IndexError => "index_error",
    }
}

pub(crate) fn concordance_fallback_label(backend: ConcordanceBackendUsed) -> &'static str {
    match backend {
        ConcordanceBackendUsed::ScanFallback { reason } => {
            concordance_fallback_reason_label(reason)
        }
        ConcordanceBackendUsed::Scan | ConcordanceBackendUsed::Index => "none",
    }
}

#[cfg(test)]
pub(crate) fn concordance_backend_used_is_index_error_fallback(
    backend: ConcordanceBackendUsed,
) -> bool {
    matches!(
        backend,
        ConcordanceBackendUsed::ScanFallback {
            reason: ConcordanceFallbackReason::IndexError,
        }
    )
}

pub(crate) fn concordance_row_identity(row: &ConcordanceRow) -> String {
    format!(
        "document_id={},token_ordinal={},node_len={}",
        row.document_index, row.token_index, row.node_token_length
    )
}

pub(crate) fn concordance_materialised_row_count(result: &ConcordanceResult) -> usize {
    result.rows.as_ref().map_or(0, Vec::len)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceAuditDifference {
    pub(crate) field: &'static str,
    pub(crate) row_index: Option<usize>,
    pub(crate) scan: String,
    pub(crate) index: String,
}

pub(crate) fn concordance_audit_difference(
    field: &'static str,
    scan: impl Into<String>,
    index: impl Into<String>,
) -> ConcordanceAuditDifference {
    ConcordanceAuditDifference {
        field,
        row_index: None,
        scan: scan.into(),
        index: index.into(),
    }
}

pub(crate) fn compare_concordance_results(
    scan: &ConcordanceResult,
    index: &ConcordanceResult,
) -> Option<ConcordanceAuditDifference> {
    if scan.total_hits != index.total_hits {
        return Some(concordance_audit_difference(
            "total_hits",
            scan.total_hits.to_string(),
            index.total_hits.to_string(),
        ));
    }
    if scan.documents_with_hits != index.documents_with_hits {
        return Some(concordance_audit_difference(
            "documents_with_hits",
            scan.documents_with_hits.to_string(),
            index.documents_with_hits.to_string(),
        ));
    }
    if scan.rows.is_some() != index.rows.is_some() {
        return Some(concordance_audit_difference(
            "rows_included",
            scan.rows.is_some().to_string(),
            index.rows.is_some().to_string(),
        ));
    }
    let scan_rows = scan.rows.as_deref().unwrap_or_default();
    let index_rows = index.rows.as_deref().unwrap_or_default();
    if scan_rows.len() != index_rows.len() {
        return Some(concordance_audit_difference(
            "rows_len",
            scan_rows.len().to_string(),
            index_rows.len().to_string(),
        ));
    }

    for (row_index, (scan_row, index_row)) in scan_rows.iter().zip(index_rows).enumerate() {
        let scan_identity = concordance_row_identity(scan_row);
        let index_identity = concordance_row_identity(index_row);
        if scan_identity != index_identity {
            return Some(ConcordanceAuditDifference {
                field: "row_identity",
                row_index: Some(row_index),
                scan: scan_identity,
                index: index_identity,
            });
        }
        if scan_row != index_row {
            return Some(ConcordanceAuditDifference {
                field: "row_payload",
                row_index: Some(row_index),
                scan: scan_identity,
                index: index_identity,
            });
        }
    }

    if scan.top_document_share.to_bits() != index.top_document_share.to_bits() {
        return Some(concordance_audit_difference(
            "top_document_share",
            format!("{:.6}", scan.top_document_share),
            format!("{:.6}", index.top_document_share),
        ));
    }
    if scan.distribution.is_some() != index.distribution.is_some() {
        return Some(concordance_audit_difference(
            "distribution_included",
            scan.distribution.is_some().to_string(),
            index.distribution.is_some().to_string(),
        ));
    }
    let Some(scan_distribution) = scan.distribution.as_ref() else {
        return (scan != index).then(|| {
            concordance_audit_difference(
                "result",
                "differs_after_checked_fields",
                "differs_after_checked_fields",
            )
        });
    };
    let index_distribution = index
        .distribution
        .as_ref()
        .expect("distribution presence checked above");
    if scan_distribution.total_documents != index_distribution.total_documents {
        return Some(concordance_audit_difference(
            "distribution_total_documents",
            scan_distribution.total_documents.to_string(),
            index_distribution.total_documents.to_string(),
        ));
    }
    if scan_distribution.items.len() != index_distribution.items.len() {
        return Some(concordance_audit_difference(
            "distribution_items_len",
            scan_distribution.items.len().to_string(),
            index_distribution.items.len().to_string(),
        ));
    }
    for (item_index, (scan_item, index_item)) in scan_distribution
        .items
        .iter()
        .zip(&index_distribution.items)
        .enumerate()
    {
        let scan_summary = format!(
            "document_id={},count={},tokens={}",
            scan_item.document_id, scan_item.count, scan_item.token_count
        );
        let index_summary = format!(
            "document_id={},count={},tokens={}",
            index_item.document_id, index_item.count, index_item.token_count
        );
        if scan_summary != index_summary {
            return Some(ConcordanceAuditDifference {
                field: "distribution_item",
                row_index: Some(item_index),
                scan: scan_summary,
                index: index_summary,
            });
        }
    }
    if scan != index {
        return Some(concordance_audit_difference(
            "result",
            "differs_after_checked_fields",
            "differs_after_checked_fields",
        ));
    }

    None
}

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

pub(crate) fn concordance_default_index_failure_state_label(
    state: ConcordanceDefaultIndexFailureState,
) -> &'static str {
    match state {
        ConcordanceDefaultIndexFailureState::RetryPending => "retry_pending",
        ConcordanceDefaultIndexFailureState::Disabled => "disabled",
    }
}

pub(crate) fn concordance_default_index_disabled_for_key(
    context_state: &ConcordanceIndexContextHandle,
    key: &ConcordanceIndexContextKey,
) -> bool {
    context_state
        .lock()
        .unwrap()
        .default_index_disabled_for(key)
}

pub(crate) fn record_concordance_default_index_error(
    context_state: &ConcordanceIndexContextHandle,
    key: &ConcordanceIndexContextKey,
) -> ConcordanceDefaultIndexFailureState {
    context_state
        .lock()
        .unwrap()
        .record_default_index_error(key)
}

pub(crate) fn search_concordance_documents_with_warm_index(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
) -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError> {
    let mut cache = context_state.lock().unwrap();
    if cache
        .entry
        .as_ref()
        .is_none_or(|cached_entry| cached_entry.key != key)
    {
        let context = cache
            .cache_root
            .clone()
            .map(ConcordanceSearchContext::with_cache_root)
            .unwrap_or_default();
        cache.entry = Some(ConcordanceIndexContextEntry {
            key: key.clone(),
            context,
        });
    }
    let entry = cache
        .entry
        .as_mut()
        .expect("concordance index context exists after insertion");
    let routed = search_concordance_documents_routed(
        documents,
        query,
        tokenization_config,
        &mut entry.context,
        ConcordanceBackend::Index,
    )?;
    if routed.backend_used == ConcordanceBackendUsed::Index {
        cache.clear_default_index_failure_for(&key);
    }
    Ok(routed)
}

pub(crate) fn search_concordance_with_warm_index_provider<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    resolve: Resolve,
    report_progress: Progress,
) -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress),
{
    let mut cache = context_state.lock().unwrap();
    if cache
        .entry
        .as_ref()
        .is_none_or(|cached_entry| cached_entry.key != key)
    {
        let context = cache
            .cache_root
            .clone()
            .map(ConcordanceSearchContext::with_cache_root)
            .unwrap_or_default();
        cache.entry = Some(ConcordanceIndexContextEntry {
            key: key.clone(),
            context,
        });
    }
    let entry = cache.entry.as_mut().expect("inserted above");
    let routed = search_concordance_documents_routed_provider(
        document_slots,
        query,
        tokenization_config,
        &mut entry.context,
        resolve,
        report_progress,
    )?;
    cache.clear_default_index_failure_for(&key);
    Ok(routed)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn aggregate_concordance_collocations_with_warm_index_provider<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::index::SurfaceIndexCancellationToken,
    resolve: Resolve,
    report_progress: Progress,
) -> Result<RoutedConcordanceCollocationAggregate, corpuswright_core::analysis::ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress),
{
    let mut cache = context_state.lock().unwrap();
    if cache
        .entry
        .as_ref()
        .is_none_or(|cached_entry| cached_entry.key != key)
    {
        let context = cache
            .cache_root
            .clone()
            .map(ConcordanceSearchContext::with_cache_root)
            .unwrap_or_default();
        cache.entry = Some(ConcordanceIndexContextEntry {
            key: key.clone(),
            context,
        });
    }
    let entry = cache.entry.as_mut().expect("inserted above");
    let routed = aggregate_concordance_collocations_routed_provider(
        document_slots,
        query,
        tokenization_config,
        &mut entry.context,
        cancellation,
        resolve,
        report_progress,
    )?;
    cache.clear_default_index_failure_for(&key);
    Ok(routed)
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    search_index: SearchIndex,
) -> Result<ConcordanceSearchOutcome, String>
where
    SearchIndex:
        FnOnce(
            &[ConcordanceDocument],
            &ConcordanceQuery,
            &corpuswright_core::tokenization::TokenizationConfig,
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    search_scan: SearchScan,
    search_index: SearchIndex,
) -> Result<ConcordanceSearchOutcome, String>
where
    SearchScan: Fn(
        &[ConcordanceDocument],
        &ConcordanceQuery,
        &corpuswright_core::tokenization::TokenizationConfig,
    ) -> Result<ConcordanceResult, corpuswright_core::analysis::ConcordanceError>,
    SearchIndex:
        FnOnce(
            &[ConcordanceDocument],
            &ConcordanceQuery,
            &corpuswright_core::tokenization::TokenizationConfig,
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::index::SurfaceIndexCancellationToken,
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::index::SurfaceIndexCancellationToken,
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
        &corpuswright_core::tokenization::TokenizationConfig,
        &corpuswright_core::index::SurfaceIndexCancellationToken,
        Resolve,
        Progress,
    ) -> Result<
        ConcordanceCollocationAggregate,
        corpuswright_core::analysis::ConcordanceError,
    >,
    SearchIndex: FnOnce(
        usize,
        &ConcordanceCollocationQuery,
        &corpuswright_core::tokenization::TokenizationConfig,
        &ConcordanceIndexContextHandle,
        ConcordanceIndexContextKey,
        &corpuswright_core::index::SurfaceIndexCancellationToken,
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

pub(crate) fn compare_concordance_collocation_aggregates(
    scan: &ConcordanceCollocationAggregate,
    index: &ConcordanceCollocationAggregate,
) -> Option<ConcordanceAuditDifference> {
    if scan.signature != index.signature {
        return Some(concordance_audit_difference(
            "collocation_signature",
            format!("{:?}", scan.signature),
            format!("{:?}", index.signature),
        ));
    }
    if scan.f_node != index.f_node {
        return Some(concordance_audit_difference(
            "collocation_f_node",
            scan.f_node.to_string(),
            index.f_node.to_string(),
        ));
    }
    if scan.r1 != index.r1 {
        return Some(concordance_audit_difference(
            "collocation_r1",
            format!("{:?}", scan.r1),
            format!("{:?}", index.r1),
        ));
    }
    if scan.n != index.n {
        return Some(concordance_audit_difference(
            "collocation_n",
            scan.n.to_string(),
            index.n.to_string(),
        ));
    }
    let semantic_candidates = |aggregate: &ConcordanceCollocationAggregate| {
        aggregate
            .candidate_counts
            .iter()
            .filter_map(|(symbol_id, counts)| {
                let index = *symbol_id as usize;
                Some((
                    aggregate.symbols.get(index)?.clone(),
                    (counts.clone(), *aggregate.collocate_frequencies.get(index)?),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let scan_candidates = semantic_candidates(scan);
    let index_candidates = semantic_candidates(index);
    if scan_candidates != index_candidates {
        return Some(concordance_audit_difference(
            "collocation_candidates",
            format!("{:?}", scan_candidates),
            format!("{:?}", index_candidates),
        ));
    }

    None
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
    tokenization_config: &corpuswright_core::tokenization::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::index::SurfaceIndexCancellationToken,
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
