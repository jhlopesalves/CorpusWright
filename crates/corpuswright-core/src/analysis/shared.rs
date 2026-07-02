use std::collections::HashSet;
use std::sync::LazyLock;

pub(super) const MAX_QUERY_WINDOW: usize = 500;
/// Heuristic DP threshold below which an item is treated as broadly dispersed.
///
/// Plain Gries-style DP is unnormalised here: its upper bound depends on the
/// document-size shares, so these labels are corpus-internal orientation aids,
/// not cross-corpus cut-offs.
pub(super) const ANNOTATED_DP_BROADLY_DISPERSED_MAX: f64 = 0.30;
/// Heuristic DP threshold from which an item is treated as highly concentrated.
pub(super) const ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN: f64 = 0.70;

pub(super) static ENGLISH_STOPWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "a about above after again against all am an and any are as at be because been before \
     being below between both but by can did do does doing down during each few for from \
     further had has have having he her here hers herself him himself his how i if in into \
     is it its itself just me more most my myself no nor not now of off on once only or \
     other our ours ourselves out over own same she should so some such than that the their \
     theirs them themselves then there these they this those through to too under until up \
     very was we were what when where which while who whom why will with you your yours \
     yourself yourselves"
        .split_whitespace()
        .collect()
});
