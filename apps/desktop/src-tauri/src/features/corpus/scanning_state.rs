use crate::runtime::cancellation::ActiveCancellation;

pub(crate) struct ScanState {
    pub(crate) cancel: ActiveCancellation,
}
