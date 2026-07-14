use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) struct AnnotationBuildState {
    pub(crate) cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AnnotationBuildState {
    pub(crate) fn new() -> Self {
        Self {
            cancel: Mutex::new(None),
        }
    }

    pub(crate) fn begin(&self) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut active = self.cancel.lock().unwrap();
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        cancel
    }

    pub(crate) fn cancel(&self) {
        if let Some(cancel) = self.cancel.lock().unwrap().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn finish(&self, token: &Arc<AtomicBool>) {
        let mut active = self.cancel.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|cancel| Arc::ptr_eq(cancel, token))
        {
            *active = None;
        }
    }
}
