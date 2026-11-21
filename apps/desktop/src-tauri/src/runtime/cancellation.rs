//! Ownership for at most one active cooperative-cancellation job.

use crate::runtime::sync::lock_recover;
use corpuswright_core::CancellationToken;
use std::sync::Mutex;

const CANCELLATION_RESOURCE: &str = "cancellation state";

#[derive(Default)]
pub(crate) struct ActiveCancellation {
    active: Mutex<Option<CancellationToken>>,
}

impl ActiveCancellation {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Starts a distinct job and cancels any job this owner previously tracked.
    pub(crate) fn begin(&self) -> CancellationToken {
        let next = CancellationToken::new();
        let mut active = lock_recover(&self.active, CANCELLATION_RESOURCE);
        if let Some(previous) = active.replace(next.clone()) {
            previous.cancel();
        }
        next
    }

    pub(crate) fn cancel(&self) {
        if let Some(active) = lock_recover(&self.active, CANCELLATION_RESOURCE).as_ref() {
            active.cancel();
        }
    }

    /// Finishes only the supplied job, so stale completions cannot clear a newer token.
    pub(crate) fn finish(&self, token: &CancellationToken) {
        let mut active = lock_recover(&self.active, CANCELLATION_RESOURCE);
        if active
            .as_ref()
            .is_some_and(|current| current.same_job(token))
        {
            *active = None;
        }
    }

    pub(crate) fn run<Result>(
        &self,
        operation: impl FnOnce(&CancellationToken) -> Result,
    ) -> Result {
        let token = self.begin();
        let result = operation(&token);
        self.finish(&token);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_cancels_old_job_and_stale_finish_preserves_new_job() {
        let owner = ActiveCancellation::new();
        let first = owner.begin();
        let second = owner.begin();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());

        owner.finish(&first);
        owner.cancel();
        assert!(second.is_cancelled());
    }
}
