//! Centralised lock-poisoning policy for shared desktop runtime state.
//!
//! Desktop commands share `RwLock` and `Mutex` state across asynchronous tasks.
//! A panic while a guard is held poisons the lock; calling `.unwrap()` would
//! cascade that panic to every later command until the application restarts.
//!
//! Two explicit policies apply:
//! - Checked access (`read_checked`, `write_checked`) returns a typed
//!   `LockPoisonedError` for authoritative multi-field state whose invariants
//!   could be broken by a mid-write panic.
//! - Recovering access (`read_recover`, `write_recover`, `write_reset`,
//!   `lock_recover`, `lock_recover_with`) recovers with
//!   `PoisonError::into_inner` for single-value slots, ephemeral caches, or
//!   full-state resets where invariants remain valid or are restored before use.

use crate::runtime::error::DesktopErrorClass;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Typed error returned when an authoritative desktop state lock is poisoned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LockPoisonedError {
    resource: &'static str,
}

impl LockPoisonedError {
    pub(crate) const fn new(resource: &'static str) -> Self {
        Self { resource }
    }

    pub(crate) const fn class(&self) -> DesktopErrorClass {
        DesktopErrorClass::InternalInvariant
    }

    pub(crate) fn into_command_error(self) -> String {
        self.to_string()
    }
}

impl fmt::Display for LockPoisonedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Internal desktop state ({}) became unavailable after an unexpected error. Clear or reload the corpus before trying again.",
            self.resource
        )
    }
}

impl std::error::Error for LockPoisonedError {}

impl From<LockPoisonedError> for String {
    fn from(error: LockPoisonedError) -> Self {
        error.into_command_error()
    }
}

fn log_poisoned(resource: &'static str, action: &'static str, class: DesktopErrorClass) {
    eprintln!(
        "[desktop-error] boundary=lock_poison resource={resource} action={action} code={}",
        class.code()
    );
}

/// Acquires a read lock on authoritative state, returning a typed error if poisoned.
pub(crate) fn read_checked<'a, T>(
    lock: &'a RwLock<T>,
    resource: &'static str,
) -> Result<RwLockReadGuard<'a, T>, LockPoisonedError> {
    lock.read().map_err(|_| {
        let error = LockPoisonedError::new(resource);
        log_poisoned(resource, "reject", error.class());
        error
    })
}

/// Acquires a write lock on authoritative state, returning a typed error if poisoned.
pub(crate) fn write_checked<'a, T>(
    lock: &'a RwLock<T>,
    resource: &'static str,
) -> Result<RwLockWriteGuard<'a, T>, LockPoisonedError> {
    lock.write().map_err(|_| {
        let error = LockPoisonedError::new(resource);
        log_poisoned(resource, "reject", error.class());
        error
    })
}

/// Acquires a read lock on state whose invariants remain valid after a panic.
pub(crate) fn read_recover<'a, T>(
    lock: &'a RwLock<T>,
    resource: &'static str,
) -> RwLockReadGuard<'a, T> {
    lock.read().unwrap_or_else(|poisoned| {
        log_poisoned(resource, "recover", DesktopErrorClass::InternalInvariant);
        PoisonError::into_inner(poisoned)
    })
}

/// Acquires a write lock on state whose invariants remain valid after a panic.
pub(crate) fn write_recover<'a, T>(
    lock: &'a RwLock<T>,
    resource: &'static str,
) -> RwLockWriteGuard<'a, T> {
    lock.write().unwrap_or_else(|poisoned| {
        log_poisoned(resource, "recover", DesktopErrorClass::InternalInvariant);
        PoisonError::into_inner(poisoned)
    })
}

/// Acquires a write lock to overwrite every field with a fresh valid state and clears poison.
pub(crate) fn write_reset<'a, T>(
    lock: &'a RwLock<T>,
    resource: &'static str,
) -> RwLockWriteGuard<'a, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log_poisoned(resource, "reset", DesktopErrorClass::InternalInvariant);
            lock.clear_poison();
            PoisonError::into_inner(poisoned)
        }
    }
}

/// Acquires a mutex lock on state whose invariants remain valid after a panic.
pub(crate) fn lock_recover<'a, T>(lock: &'a Mutex<T>, resource: &'static str) -> MutexGuard<'a, T> {
    lock.lock().unwrap_or_else(|poisoned| {
        log_poisoned(resource, "recover", DesktopErrorClass::InternalInvariant);
        PoisonError::into_inner(poisoned)
    })
}

/// Acquires a mutex lock, restores invariants on poison, and clears the poison flag.
pub(crate) fn lock_recover_with<'a, T>(
    lock: &'a Mutex<T>,
    resource: &'static str,
    on_poison: impl FnOnce(&mut T),
) -> MutexGuard<'a, T> {
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log_poisoned(resource, "reset", DesktopErrorClass::InternalInvariant);
            let mut guard = PoisonError::into_inner(poisoned);
            on_poison(&mut guard);
            lock.clear_poison();
            guard
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::error::classify_message;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    fn poison_rwlock<T>(lock: &RwLock<T>) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = lock.write().unwrap();
            panic!("simulated panic while holding write lock");
        }));
        assert!(lock.is_poisoned());
    }

    fn poison_mutex<T>(lock: &Mutex<T>) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = lock.lock().unwrap();
            panic!("simulated panic while holding mutex");
        }));
        assert!(lock.is_poisoned());
    }

    #[test]
    fn checked_rwlock_returns_typed_error_and_reset_clears_poison() {
        let lock = RwLock::new(42usize);
        poison_rwlock(&lock);

        let read_error = read_checked(&lock, "test state").unwrap_err();
        assert_eq!(read_error, LockPoisonedError::new("test state"));
        assert_eq!(read_error.class(), DesktopErrorClass::InternalInvariant);
        assert_eq!(
            classify_message(&read_error.to_string()),
            DesktopErrorClass::InternalInvariant
        );
        assert_eq!(
            write_checked(&lock, "test state").unwrap_err(),
            LockPoisonedError::new("test state")
        );

        *write_reset(&lock, "test state") = 7;
        assert!(!lock.is_poisoned());
        assert_eq!(*read_checked(&lock, "test state").unwrap(), 7);
    }

    #[test]
    fn recover_helpers_return_inner_value_without_panicking() {
        let rw = RwLock::new(Some(10usize));
        poison_rwlock(&rw);
        assert_eq!(*read_recover(&rw, "cache"), Some(10));
        *write_recover(&rw, "cache") = Some(20);
        assert_eq!(*read_recover(&rw, "cache"), Some(20));

        let mutex = Mutex::new(Some("stale"));
        poison_mutex(&mutex);
        assert_eq!(*lock_recover(&mutex, "slot"), Some("stale"));

        let guard = lock_recover_with(&mutex, "slot", |value| *value = None);
        assert_eq!(*guard, None);
        drop(guard);
        assert!(!mutex.is_poisoned());
    }
}
