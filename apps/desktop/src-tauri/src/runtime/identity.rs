//! Typed internal identities keep independent lifecycle and cache domains incomparable.
//!
//! IPC and persisted representations remain primitive. Conversion occurs only at command,
//! serialisation, and cache-construction boundaries.

macro_rules! numeric_identity {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) const fn from_ipc(value: u64) -> Self {
                Self(value)
            }

            pub(crate) const fn into_ipc(self) -> u64 {
                self.0
            }

            pub(crate) fn advance(&mut self) -> Self {
                self.0 = self.0.wrapping_add(1);
                *self
            }
        }
    };
}

numeric_identity!(CorpusVersion);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AnalysisGeneration(u64);

impl AnalysisGeneration {
    pub(crate) fn advance(&mut self) -> Self {
        self.0 = self.0.wrapping_add(1);
        *self
    }

    #[cfg(test)]
    const fn from_test_value(value: u64) -> Self {
        Self(value)
    }

    #[cfg(test)]
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
        let mut corpus = CorpusVersion::from_ipc(4);
        let mut analysis = AnalysisGeneration::from_test_value(4);
        assert_eq!(corpus.advance().into_ipc(), 5);
        assert_eq!(analysis.advance().test_value(), 5);
        // The types deliberately have no cross-domain PartialEq implementation. Primitive
        // comparison is possible only after an explicit IPC-boundary conversion.
        assert_eq!(corpus.into_ipc(), analysis.test_value());
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
