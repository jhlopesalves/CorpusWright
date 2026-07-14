//! Scan/index semantic parity comparison ownership.

use corpuswright_core::analysis::{
    ConcordanceCollocationAggregate, ConcordanceResult, ConcordanceRow,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceAuditDifference {
    pub(crate) field: &'static str,
    pub(crate) row_index: Option<usize>,
    pub(crate) scan: String,
    pub(crate) index: String,
}

fn difference(
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

fn row_identity(row: &ConcordanceRow) -> String {
    format!(
        "document_id={},token_ordinal={},node_len={}",
        row.document_index, row.token_index, row.node_token_length
    )
}

pub(super) fn concordance_materialised_row_count(result: &ConcordanceResult) -> usize {
    result.rows.as_ref().map_or(0, Vec::len)
}

pub(super) fn compare_concordance_results(
    scan: &ConcordanceResult,
    index: &ConcordanceResult,
) -> Option<ConcordanceAuditDifference> {
    if scan.total_hits != index.total_hits {
        return Some(difference(
            "total_hits",
            scan.total_hits.to_string(),
            index.total_hits.to_string(),
        ));
    }
    if scan.documents_with_hits != index.documents_with_hits {
        return Some(difference(
            "documents_with_hits",
            scan.documents_with_hits.to_string(),
            index.documents_with_hits.to_string(),
        ));
    }
    if scan.rows.is_some() != index.rows.is_some() {
        return Some(difference(
            "rows_included",
            scan.rows.is_some().to_string(),
            index.rows.is_some().to_string(),
        ));
    }
    let scan_rows = scan.rows.as_deref().unwrap_or_default();
    let index_rows = index.rows.as_deref().unwrap_or_default();
    if scan_rows.len() != index_rows.len() {
        return Some(difference(
            "rows_len",
            scan_rows.len().to_string(),
            index_rows.len().to_string(),
        ));
    }

    for (row_index, (scan_row, index_row)) in scan_rows.iter().zip(index_rows).enumerate() {
        let scan_identity = row_identity(scan_row);
        let index_identity = row_identity(index_row);
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
        return Some(difference(
            "top_document_share",
            format!("{:.6}", scan.top_document_share),
            format!("{:.6}", index.top_document_share),
        ));
    }
    if scan.distribution.is_some() != index.distribution.is_some() {
        return Some(difference(
            "distribution_included",
            scan.distribution.is_some().to_string(),
            index.distribution.is_some().to_string(),
        ));
    }
    let Some(scan_distribution) = scan.distribution.as_ref() else {
        return (scan != index).then(|| {
            difference(
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
        return Some(difference(
            "distribution_total_documents",
            scan_distribution.total_documents.to_string(),
            index_distribution.total_documents.to_string(),
        ));
    }
    if scan_distribution.items.len() != index_distribution.items.len() {
        return Some(difference(
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
        return Some(difference(
            "result",
            "differs_after_checked_fields",
            "differs_after_checked_fields",
        ));
    }

    None
}

pub(super) fn compare_concordance_collocation_aggregates(
    scan: &ConcordanceCollocationAggregate,
    index: &ConcordanceCollocationAggregate,
) -> Option<ConcordanceAuditDifference> {
    if scan.signature != index.signature {
        return Some(difference(
            "collocation_signature",
            format!("{:?}", scan.signature),
            format!("{:?}", index.signature),
        ));
    }
    if scan.f_node != index.f_node {
        return Some(difference(
            "collocation_f_node",
            scan.f_node.to_string(),
            index.f_node.to_string(),
        ));
    }
    if scan.r1 != index.r1 {
        return Some(difference(
            "collocation_r1",
            format!("{:?}", scan.r1),
            format!("{:?}", index.r1),
        ));
    }
    if scan.n != index.n {
        return Some(difference(
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
        return Some(difference(
            "collocation_candidates",
            format!("{:?}", scan_candidates),
            format!("{:?}", index_candidates),
        ));
    }

    None
}
