// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use tokio::sync::Notify;

/// Shared state for tracking the number of rows currently in-flight between the collector and
/// committer. The collector creates guards that increment the count; when guards are dropped
/// (by the committer after writing), the count decrements and waiters are notified.
#[derive(Clone)]
pub(super) struct InflightRows {
    counter: Arc<AtomicUsize>,
    notify: Arc<Notify>,
}

/// RAII guard that tracks pending rows. When dropped, the counter is decremented and waiters
/// are notified. This ensures rows are always accounted for, even on error paths.
pub(super) struct PendingRowsGuard {
    inflight: InflightRows,
    count: usize,
}

impl InflightRows {
    pub(super) fn new() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Returns the current number of in-flight rows.
    pub(super) fn count(&self) -> usize {
        self.counter.load(Ordering::Relaxed)
    }

    /// Creates an RAII guard that tracks `count` rows. The counter is incremented immediately;
    /// when the guard is dropped, the counter is decremented and waiters are notified.
    pub(super) fn guard(&self, count: usize) -> PendingRowsGuard {
        self.counter.fetch_add(count, Ordering::Relaxed);
        PendingRowsGuard {
            inflight: self.clone(),
            count,
        }
    }

    /// Waits until the in-flight row count drops below `max`, then awaits `f`.
    pub(super) async fn backpressured<F: Future>(&self, max: usize, f: F) -> F::Output {
        while self.counter.load(Ordering::Relaxed) >= max {
            self.notify.notified().await;
        }
        f.await
    }
}

impl PendingRowsGuard {
    /// Transfer `n` rows from this guard into a new guard. No atomic operations are performed;
    /// the total count across both guards remains constant.
    pub(super) fn split(&mut self, n: usize) -> Self {
        self.count = self.count.saturating_sub(n);
        Self {
            inflight: self.inflight.clone(),
            count: n,
        }
    }

    /// Absorb another guard's count into this one. The other guard is disarmed (its Drop becomes
    /// a no-op).
    pub(super) fn merge(&mut self, mut other: Self) {
        self.count = self.count.saturating_add(other.count);
        other.count = 0;
    }
}

impl Drop for PendingRowsGuard {
    fn drop(&mut self) {
        if self.count > 0 {
            self.inflight
                .counter
                .fetch_sub(self.count, Ordering::Relaxed);
            self.inflight.notify.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_new_and_drop() {
        let inflight = InflightRows::new();
        {
            let _guard = inflight.guard(10);
            assert_eq!(inflight.count(), 10);
        }
        assert_eq!(inflight.count(), 0);
    }

    #[test]
    fn test_guard_split_and_merge() {
        let inflight = InflightRows::new();
        let mut guard = inflight.guard(10);

        let split = guard.split(3);
        assert_eq!(inflight.count(), 10);

        let mut merged = inflight.guard(0);
        merged.merge(split);
        assert_eq!(inflight.count(), 10);

        drop(guard);
        assert_eq!(inflight.count(), 3);

        drop(merged);
        assert_eq!(inflight.count(), 0);
    }
}
