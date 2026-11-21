//! Typed internal identities keep independent lifecycle and cache domains incomparable.
//!
//! IPC and persisted representations remain primitive. Conversion occurs only at command,
//! serialisation, and cache-construction boundaries.

use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! numeric_identity {
    ($name:ident, $sequence:ident) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) const fn from_ipc(value: u64) -> Self {
                Self(value)
            }

            pub(crate) const fn into_ipc(self) -> u64 {
                self.0
            }
        }

        #[derive(Default)]
        pub(crate) struct $sequence(AtomicU64);

        impl $sequence {
            pub(crate) fn next(&self) -> $name {
                $name(self.0.fetch_add(1, Ordering::Relaxed).wrapping_add(1))
            }
        }
    };
}

numeric_identity!(CorpusVersion, CorpusVersionSequence);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AnalysisGeneration(u64);

#[derive(Default)]
pub(crate) struct AnalysisGenerationSequence(AtomicU64);

impl AnalysisGenerationSequence {
    pub(crate) fn next(&self) -> AnalysisGeneration {
        AnalysisGeneration(self.0.fetch_add(1, Ordering::Relaxed).wrapping_add(1))
    }
}

#[cfg(test)]
impl AnalysisGeneration {
    const fn test_value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CleaningConfigSignature(String);

impl CleaningConfigSignature {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProjectRootIdentity(String);

impl ProjectRootIdentity {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct IndexIdentity {
    pub(crate) corpus: CorpusVersion,
    pub(crate) cleaning: CleaningConfigSignature,
}

impl IndexIdentity {
    pub(crate) fn from_ipc(corpus_version: u64, cleaning: impl Into<String>) -> Self {
        Self {
            corpus: CorpusVersion::from_ipc(corpus_version),
            cleaning: CleaningConfigSignature::new(cleaning),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_domains_advance_independently_and_cross_only_at_ipc() {
        let corpus = CorpusVersionSequence::default();
        let analysis = AnalysisGenerationSequence::default();
        let corpus_next = corpus.next();
        let analysis_next = analysis.next();
        assert_eq!(corpus_next.into_ipc(), 1);
        assert_eq!(analysis_next.test_value(), 1);
        assert_eq!(corpus.next().into_ipc(), 2);
        // The types deliberately have no cross-domain PartialEq implementation. Primitive
        // comparison is possible only after an explicit IPC-boundary conversion.
        assert_eq!(corpus_next.into_ipc(), analysis_next.test_value());
    }

    #[test]
    fn labelled_cache_identity_preserves_every_component() {
        let first = IndexIdentity {
            corpus: CorpusVersion::from_ipc(1),
            cleaning: CleaningConfigSignature::new("a"),
        };
        assert_ne!(
            first,
            IndexIdentity {
                corpus: CorpusVersion::from_ipc(2),
                cleaning: CleaningConfigSignature::new("a"),
            }
        );
        assert_ne!(
            first,
            IndexIdentity {
                corpus: CorpusVersion::from_ipc(1),
                cleaning: CleaningConfigSignature::new("b"),
            }
        );
    }
}
