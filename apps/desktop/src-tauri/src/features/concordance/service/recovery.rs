//! Default-index retry, disable, and permitted fallback state transitions.

use super::super::state::{
    ConcordanceDefaultIndexFailureState, ConcordanceIndexContextHandle, ConcordanceIndexContextKey,
    lock_index_context,
};

pub(super) fn concordance_default_index_failure_state_label(
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
    lock_index_context(context_state).default_index_disabled_for(key)
}

pub(crate) fn record_concordance_default_index_error(
    context_state: &ConcordanceIndexContextHandle,
    key: &ConcordanceIndexContextKey,
) -> ConcordanceDefaultIndexFailureState {
    lock_index_context(context_state).record_default_index_error(key)
}
