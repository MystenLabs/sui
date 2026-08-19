// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-thread execution permit, released when the thread blocks.
//!
//! Sui runs transaction execution on a pool of blocking threads whose concurrency is
//! capped by a semaphore permit. Execution may need to block waiting for a value that
//! another execution must produce (e.g. an object version). If a blocked thread keeps
//! holding its permit, and enough threads block this way, no permit is left for the
//! execution that would unblock them - a deadlock.
//!
//! To avoid this, the permit is installed on the thread with [`set_execution_permit`],
//! and the blocking sync primitives in this crate call [`release_execution_permit`] the
//! first time they must actually block (after a non-blocking `try_` check fails). The
//! permit is intentionally *not* re-acquired afterwards: this may briefly exceed the
//! configured concurrency, but the excess resolves itself as tasks complete.
//!
//! The permit is stored type-erased (`Box<dyn Send>`) so this crate need not depend on
//! the specific semaphore; releasing it simply drops the box.

use std::cell::RefCell;

thread_local! {
    static EXECUTION_PERMIT: RefCell<Option<Box<dyn Send>>> = const { RefCell::new(None) };
}

/// Guard returned by [`set_execution_permit`]. Releases the thread's permit on drop if a
/// blocking primitive has not already done so.
#[must_use = "the execution permit is released when this guard is dropped"]
pub struct ExecutionPermitGuard(());

/// Installs `permit` as the current thread's execution permit. Panics if one is already
/// installed (each blocking-pool task installs exactly one permit for the duration of
/// its run).
pub fn set_execution_permit(permit: Box<dyn Send>) -> ExecutionPermitGuard {
    EXECUTION_PERMIT.with(|slot| {
        let previous = slot.borrow_mut().replace(permit);
        assert!(
            previous.is_none(),
            "an execution permit is already installed on this thread"
        );
    });
    ExecutionPermitGuard(())
}

/// Releases (drops) the current thread's execution permit if one is installed.
/// Idempotent, and a no-op when no permit is installed. Called by blocking primitives
/// immediately before they park or spin.
pub fn release_execution_permit() {
    // Take the permit out before dropping it so its `Drop` does not run while the
    // thread-local is still borrowed (a permit's drop must not re-enter this module,
    // but taking first keeps that guarantee local).
    let permit = EXECUTION_PERMIT.with(|slot| slot.borrow_mut().take());
    drop(permit);
}

impl Drop for ExecutionPermitGuard {
    fn drop(&mut self) {
        release_execution_permit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Sets the flag when dropped, so tests can observe when the permit is released.
    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn guard_releases_on_drop() {
        let released = Arc::new(AtomicBool::new(false));
        {
            let _guard = set_execution_permit(Box::new(DropFlag(released.clone())));
            assert!(!released.load(Ordering::SeqCst));
        }
        assert!(released.load(Ordering::SeqCst));
    }

    #[test]
    fn explicit_release_drops_permit_and_guard_is_noop() {
        let released = Arc::new(AtomicBool::new(false));
        let guard = set_execution_permit(Box::new(DropFlag(released.clone())));
        release_execution_permit();
        assert!(released.load(Ordering::SeqCst), "released immediately");
        // Dropping the guard afterwards must be a harmless no-op.
        drop(guard);
    }

    #[test]
    fn release_without_permit_is_noop() {
        release_execution_permit();
    }
}
