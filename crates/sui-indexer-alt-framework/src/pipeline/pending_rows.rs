// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use tokio::sync::Notify;

/// RAII guard that tracks pending rows via an atomic counter. When the guard is dropped, the
/// counter is decremented by the guard's count. This ensures rows are always accounted for,
/// even on error paths. Notifies waiters (the broadcaster) when rows are released.
pub(crate) struct PendingRowsGuard {
    counter: Arc<AtomicUsize>,
    notify: Arc<Notify>,
    count: usize,
}

impl PendingRowsGuard {
    pub(crate) fn new(counter: Arc<AtomicUsize>, notify: Arc<Notify>, count: usize) -> Self {
        counter.fetch_add(count, Ordering::Relaxed);
        Self {
            counter,
            notify,
            count,
        }
    }

    /// Create a mock guard for tests that uses its own isolated counter and a dummy notify.
    #[cfg(test)]
    pub(crate) fn mock(count: usize) -> Self {
        let counter = Arc::new(AtomicUsize::new(0));
        counter.fetch_add(count, Ordering::Relaxed);
        Self {
            counter,
            notify: Arc::new(Notify::new()),
            count,
        }
    }

    /// Transfer `n` rows from this guard into a new guard. No atomic operations are performed;
    /// the total count across both guards remains constant.
    pub(crate) fn split(&mut self, n: usize) -> Self {
        self.count = self.count.saturating_sub(n);
        Self {
            counter: self.counter.clone(),
            notify: self.notify.clone(),
            count: n,
        }
    }

    /// Absorb another guard's count into this one. The other guard is disarmed (its Drop becomes
    /// a no-op).
    pub(crate) fn merge(&mut self, mut other: Self) {
        self.count = self.count.saturating_add(other.count);
        other.count = 0;
    }
}

impl Drop for PendingRowsGuard {
    fn drop(&mut self) {
        if self.count > 0 {
            self.counter.fetch_sub(self.count, Ordering::Relaxed);
            self.notify.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_rows_guard_new_and_drop() {
        let counter = Arc::new(AtomicUsize::new(0));
        let notify = Arc::new(Notify::new());
        {
            let _guard = PendingRowsGuard::new(counter.clone(), notify.clone(), 10);
            assert_eq!(counter.load(Ordering::Relaxed), 10);
        }
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_pending_rows_guard_split_and_merge() {
        let counter = Arc::new(AtomicUsize::new(0));
        let notify = Arc::new(Notify::new());
        let mut guard = PendingRowsGuard::new(counter.clone(), notify.clone(), 10);

        let split = guard.split(3);
        assert_eq!(counter.load(Ordering::Relaxed), 10);

        let mut merged = PendingRowsGuard::new(counter.clone(), notify.clone(), 0);
        merged.merge(split);
        assert_eq!(counter.load(Ordering::Relaxed), 10);

        drop(guard);
        assert_eq!(counter.load(Ordering::Relaxed), 3);

        drop(merged);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
