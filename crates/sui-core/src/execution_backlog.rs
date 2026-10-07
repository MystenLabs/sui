// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use tokio::time::Instant;

/// The execution driver's view of its own progress: how many transactions it has
/// executed and how long the oldest ready transaction has been waiting to start.
/// Published by the driver and read by the consensus transaction pool to pace user
/// transaction admission.
#[derive(Default)]
pub struct ExecutionBacklog {
    /// Transactions this authority has executed successfully.
    pub executed_transactions: AtomicU64,

    /// When the oldest waiting transaction was last observed, and how long it had been
    /// waiting at that moment. None when nothing is waiting.
    oldest: Mutex<Option<(Instant, Duration)>>,
}

impl ExecutionBacklog {
    /// Records the ready time of the longest-waiting transaction not yet admitted to
    /// execution, or None when the driver has nothing waiting.
    pub fn set_oldest_ready(&self, ready_at: Option<Instant>) {
        let now = Instant::now();
        *self.oldest.lock() = ready_at.map(|t| (now, now.saturating_duration_since(t)));
    }

    /// Records that the oldest waiting transaction has been waiting for `wait` as of now.
    pub fn set_oldest_ready_wait(&self, wait: Duration) {
        *self.oldest.lock() = Some((Instant::now(), wait));
    }

    /// How long the oldest ready transaction has been waiting to start executing, or None
    /// when nothing is waiting.
    pub fn oldest_ready_wait(&self) -> Option<Duration> {
        let (observed_at, wait_then) = (*self.oldest.lock())?;
        Some(wait_then + Instant::now().saturating_duration_since(observed_at))
    }
}
