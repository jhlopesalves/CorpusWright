//! Shared cooperative-cancellation ownership.
//!
//! Algorithms retain domain-specific checkpoint cadence and may continue to accept `&AtomicBool`
//! while owners pass this token through deref coercion. The token standardises lifecycle, identity,
//! and the error returned by explicit checkpoints.

use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("operation cancelled")
    }
}

impl std::error::Error for Cancelled {}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn same_job(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn into_atomic(self) -> Arc<AtomicBool> {
        self.0
    }
}

impl Deref for CancellationToken {
    type Target = AtomicBool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Arc<AtomicBool>> for CancellationToken {
    fn from(value: Arc<AtomicBool>) -> Self {
        Self(value)
    }
}

impl From<CancellationToken> for Arc<AtomicBool> {
    fn from(value: CancellationToken) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_cancellation_and_independent_jobs_do_not() {
        let first = CancellationToken::new();
        let clone = first.clone();
        let second = CancellationToken::new();
        assert!(first.same_job(&clone));
        assert!(!first.same_job(&second));
        assert_eq!(first.check(), Ok(()));
        clone.cancel();
        assert!(first.is_cancelled());
        assert_eq!(first.check(), Err(Cancelled));
        assert!(!second.is_cancelled());
    }
}
