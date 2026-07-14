use corpuswright_core::preparation::repeated_artifacts::CancellationFlag;

pub(crate) struct ScanState {
    pub(crate) cancel: CancellationFlag,
}
