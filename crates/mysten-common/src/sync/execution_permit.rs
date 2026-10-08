// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-thread execution-capacity permit, released when the thread blocks.
//!
//! Sui runs transaction execution on a pool of blocking threads whose concurrency is
//! capped by the causal-admission controller. Execution may block waiting for a value
//! another execution must produce. If every blocked thread keeps consuming capacity,
//! the producer may never be admitted. The execution driver installs a type-erased
//! permit on the blocking thread, and the blocking sync primitives release it before
//! parking. The transaction's causal index remains live until execution completes.

use std::cell::RefCell;

thread_local! {
    static EXECUTION_PERMIT: RefCell<Option<Box<dyn Send>>> = const { RefCell::new(None) };
}

/// Guard returned by [`set_execution_permit`]. Releases the thread's capacity on drop
/// if a blocking primitive has not already done so.
#[must_use = "the execution permit is released when this guard is dropped"]
pub struct ExecutionPermitGuard(());

/// Installs `permit` as the current thread's execution-capacity permit.
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

/// Releases the current thread's execution capacity, if any. Idempotent.
pub fn release_execution_permit() {
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
    fn explicit_release_is_idempotent() {
        let released = Arc::new(AtomicBool::new(false));
        let guard = set_execution_permit(Box::new(DropFlag(released.clone())));
        release_execution_permit();
        assert!(released.load(Ordering::SeqCst));
        drop(guard);
    }

    #[test]
    fn release_without_permit_is_noop() {
        release_execution_permit();
    }
}
