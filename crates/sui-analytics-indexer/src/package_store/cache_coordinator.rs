// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A zero-allocation coordinator that lets one “priming” handler publish
//! a monotonically-increasing (epoch, checkpoint) pair while any number of
//! downstream handlers await a particular pair before they start work.
//!
//! •   Single *atomic compare-and-swap* per update (via `crossbeam::atomic::AtomicCell`).
//! •   O(1) memory: one tuple in an `AtomicCell` plus a single-slot `watch` channel.
//! •   No nightly features, works on stable Rust.

use crossbeam::atomic::AtomicCell;
use std::sync::Arc;
use tokio::sync::watch;

/// Lexicographic comparison helper: (e1, c1) ≥ (e2, c2)?
#[inline]
fn ge(e1: u64, c1: u64, e2: u64, c2: u64) -> bool {
    e1 > e2 || (e1 == e2 && c1 >= c2)
}

/// Coordinator shared between the package-cache priming handler and all
/// analytics handlers that need to wait for the cache to be up-to-date.
#[derive(Clone)]
pub struct CacheReadyCoordinator {
    latest: Arc<AtomicCell<(u64, u64)>>,
    tx: watch::Sender<(u64, u64)>,
    rx: watch::Receiver<(u64, u64)>,
}

#[allow(clippy::new_without_default)]
impl CacheReadyCoordinator {
    /// Create a new coordinator initialised to `(epoch 0, checkpoint 0)`.
    pub fn new() -> Self {
        let (tx, rx) = watch::channel((0, 0));
        Self {
            latest: Arc::new(AtomicCell::new((0, 0))),
            tx,
            rx,
        }
    }

    /// Call from the *priming handler* **after** the package cache contains
    /// all objects for the given `(epoch, checkpoint)`.
    ///
    /// Guarantees the public value never moves **backwards**.
    pub fn mark_ready(&self, epoch: u64, ckpt: u64) {
        loop {
            let cur = self.latest.load();
            // Already at or ahead of the requested value → nothing to do.
            if ge(cur.0, cur.1, epoch, ckpt) {
                return;
            }
            // Attempt to publish the newer value.
            if self.latest.compare_exchange(cur, (epoch, ckpt)).is_ok() {
                // Broadcast to all waiters (ignore error if every receiver dropped).
                let _ = self.tx.send_replace((epoch, ckpt));
                return;
            }
            // Another thread won the race → retry.
        }
    }

    /// Async wait until the *published* pair is **≥ (epoch, ckpt)**.
    pub async fn wait(&self, epoch: u64, ckpt: u64) {
        // Fast path using only atomics.
        if ge(self.latest.load().0, self.latest.load().1, epoch, ckpt) {
            return;
        }

        // Slow path: subscribe to the watch channel.
        let mut rx = self.rx.clone();
        loop {
            let (e, c) = *rx.borrow();
            if ge(e, c, epoch, ckpt) {
                return;
            }
            if rx.changed().await.is_err() {
                // Sender dropped: treat everything as ready.
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn monotone_updates_and_wait() {
        let coord = CacheReadyCoordinator::new();

        // Spawn a waiter for (2, 5).
        let c2 = coord.clone();
        let waiter = tokio::spawn(async move {
            c2.wait(2, 5).await;
        });

        // Publish out of order: (1, 9) then (2, 4) → neither should wake the waiter.
        coord.mark_ready(1, 9);
        coord.mark_ready(2, 4);

        // Publish (2, 5) → waiter should finish.
        coord.mark_ready(2, 5);
        waiter.await.unwrap();

        // Publish older value: should be ignored.
        coord.mark_ready(1, 0);
        assert_eq!(coord.latest.load(), (2, 5));

        // Faster wait: already satisfied.
        coord.wait(1, 1).await;
    }

    #[tokio::test]
    async fn broadcast_to_many() {
        let coord = CacheReadyCoordinator::new();
        let mut handles = Vec::new();

        // Ten waiters for (3, 3).
        for _ in 0..10 {
            let c = coord.clone();
            handles.push(tokio::spawn(async move {
                c.wait(3, 3).await;
            }));
        }

        // Advance slowly.
        coord.mark_ready(1, 0);
        sleep(Duration::from_millis(10)).await;
        coord.mark_ready(2, 10);
        sleep(Duration::from_millis(10)).await;
        coord.mark_ready(3, 3);

        for h in handles {
            h.await.unwrap();
        }
    }
}
