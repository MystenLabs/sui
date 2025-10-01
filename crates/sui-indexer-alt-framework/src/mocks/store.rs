// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;

use anyhow::ensure;
use async_trait::async_trait;
use scoped_futures::ScopedBoxFuture;
use tokio::time::Duration;

use crate::store::{
    CommitterWatermark, Connection, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};

#[derive(Default, Clone)]
pub struct MockWatermark {
    pub epoch_hi_inclusive: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi: u64,
    pub timestamp_ms_hi_inclusive: u64,
    pub reader_lo: u64,
    pub pruner_timestamp: u64,
    pub pruner_hi: u64,
}

/// Configuration for simulating connection failures in tests
#[derive(Default)]
pub struct ConnectionFailure {
    /// Number of failures before connection succeeds
    pub connection_failure_attempts: usize,
    /// Delay in milliseconds for each connection attempt (applied even when connection fails)
    pub connection_delay_ms: u64,
    /// Counter for tracking total connection attempts
    pub connection_attempts: usize,
}

/// Configuration for simulating failures in tests
#[derive(Default)]
pub struct Failures {
    /// Number of failures to simulate before allowing success
    pub failures: usize,
    /// Counter for tracking total transaction attempts
    pub attempts: AtomicUsize,
}

/// A mock store for testing. Represents an indexer with a single pipeline. It maintains a map of
/// checkpoint sequence numbers to transaction sequence numbers, and a watermark that can be used to
/// test the watermark task.
#[derive(Clone, Default)]
pub struct MockStore {
    /// Tracks various watermark states (committer, reader, pruner). This value can be optional
    /// until a checkpoint is committed.
    pub watermark: Arc<Mutex<Option<MockWatermark>>>,
    /// Stores the actual data, mapping checkpoint sequence numbers to transaction sequence numbers
    pub data: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
    /// Tracks the order of checkpoint processing for testing sequential processing
    /// Each entry is the checkpoint number that was processed
    pub sequential_checkpoint_data: Arc<Mutex<Vec<u64>>>,
    /// Controls pruning failure simulation for testing retry behavior.
    /// Maps from [from_checkpoint, to_checkpoint_exclusive) to Failures struct.
    /// Thread-safe for concurrent access during pruning operations.
    pub prune_failure_attempts: Arc<DashMap<(u64, u64), Failures>>,
    /// Configuration for simulating connection failures in tests
    pub connection_failure: Arc<Mutex<ConnectionFailure>>,
    /// Number of remaining failures for set_reader_watermark operation
    pub set_reader_watermark_failure_attempts: Arc<Mutex<usize>>,
    /// Configuration for simulating transaction failures in tests
    pub transaction_failures: Arc<Failures>,
    /// Configuration for simulating commit failures in tests
    pub commit_failures: Arc<Failures>,
    /// Configuration for simulating commit watermark failures in tests
    pub commit_watermark_failures: Arc<Failures>,
    /// Delay in milliseconds for each transaction commit
    pub commit_delay_ms: u64,
}

#[derive(Clone)]
pub struct MockConnection<'c>(pub &'c MockStore);

#[async_trait]
impl Connection for MockConnection<'_> {
    async fn committer_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> Result<Option<CommitterWatermark>, anyhow::Error> {
        let watermark = self.0.watermark();
        Ok(watermark.map(|w| CommitterWatermark {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }))
    }

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> Result<Option<ReaderWatermark>, anyhow::Error> {
        let watermark = self.0.watermark();
        Ok(watermark.map(|w| ReaderWatermark {
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            reader_lo: w.reader_lo,
        }))
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>, anyhow::Error> {
        let watermark = self.0.watermark();
        Ok(watermark.map(|w| {
            let elapsed_ms = w.pruner_timestamp as i64
                - SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
            let wait_for_ms = delay.as_millis() as i64 + elapsed_ms;
            PrunerWatermark {
                pruner_hi: w.pruner_hi,
                reader_lo: w.reader_lo,
                wait_for_ms,
            }
        }))
    }

    async fn set_committer_watermark(
        &mut self,
        _pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        // Check if we should simulate a commit failure
        let prev = self
            .0
            .commit_watermark_failures
            .attempts
            .fetch_add(1, Ordering::Relaxed);
        ensure!(
            prev >= self.0.commit_watermark_failures.failures,
            "Commit failed, remaining failures: {}",
            self.0.commit_watermark_failures.failures - prev
        );

        let mut curr = self.0.watermark.lock().unwrap();
        *curr = Some(MockWatermark {
            epoch_hi_inclusive: watermark.epoch_hi_inclusive,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
            tx_hi: watermark.tx_hi,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
            reader_lo: curr.as_ref().map(|w| w.reader_lo).unwrap_or(0),
            pruner_timestamp: curr.as_ref().map(|w| w.pruner_timestamp).unwrap_or(0),
            pruner_hi: curr.as_ref().map(|w| w.pruner_hi).unwrap_or(0),
        });
        Ok(true)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        // Check for set_reader_watermark failure simulation
        let should_fail = {
            let mut attempts = self.0.set_reader_watermark_failure_attempts.lock().unwrap();
            if *attempts > 0 {
                *attempts -= 1;
                true
            } else {
                false
            }
        };

        if should_fail {
            return Err(anyhow::anyhow!("set_reader_watermark failed"));
        }

        let mut curr = self.0.watermark.lock().unwrap();
        *curr = Some(MockWatermark {
            reader_lo,
            pruner_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            ..curr.as_ref().unwrap().clone()
        });
        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        let mut curr = self.0.watermark.lock().unwrap();
        *curr = Some(MockWatermark {
            pruner_hi,
            ..curr.as_ref().unwrap().clone()
        });
        Ok(true)
    }
}

#[async_trait]
impl Store for MockStore {
    type Connection<'c> = MockConnection<'c>;

    async fn connect(&self) -> anyhow::Result<Self::Connection<'_>> {
        // Check for connection failure simulation and increment attempts counter
        let (should_fail, delay_ms) = {
            let mut failure = self.connection_failure.lock().unwrap();
            failure.connection_attempts += 1;

            let should_fail = if failure.connection_failure_attempts > 0 {
                failure.connection_failure_attempts -= 1;
                true
            } else {
                false
            };

            (should_fail, failure.connection_delay_ms)
        };

        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        ensure!(!should_fail, "Connection failed");

        Ok(MockConnection(self))
    }
}

#[async_trait]
impl TransactionalStore for MockStore {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        // Check if we should simulate a transaction failure
        let prev = self
            .transaction_failures
            .attempts
            .fetch_add(1, Ordering::Relaxed);
        ensure!(
            prev >= self.transaction_failures.failures,
            "Transaction failed, remaining failures: {}",
            self.transaction_failures.failures - prev
        );

        let mut conn = self.connect().await?;
        f(&mut conn).await
    }
}

impl MockStore {
    /// Commits data to the mock store, handling delays and simulated failures
    pub async fn commit_data(
        &self,
        values: std::collections::HashMap<u64, Vec<u64>>,
    ) -> anyhow::Result<usize> {
        // Apply commit delay if configured
        if self.commit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.commit_delay_ms)).await;
        }

        // Check for commit failure simulation
        let prev = self
            .commit_failures
            .attempts
            .fetch_add(1, Ordering::Relaxed);
        ensure!(
            prev >= self.commit_failures.failures,
            "Transaction failed, remaining failures: {}",
            self.commit_failures.failures - prev
        );

        // Store the data
        let mut total_count = 0;
        {
            let mut data = self.data.lock().unwrap();
            for (checkpoint, checkpoint_values) in values {
                total_count += checkpoint_values.len();
                data.entry(checkpoint)
                    .or_default()
                    .extend(checkpoint_values);
            }
        }

        Ok(total_count)
    }

    /// Prunes data for the given checkpoints, handling failure simulation
    pub fn prune_data(&self, from: u64, to_exclusive: u64) -> anyhow::Result<usize> {
        let should_fail = self
            .prune_failure_attempts
            .get(&(from, to_exclusive))
            .is_some_and(|f| f.attempts.fetch_add(1, Ordering::Relaxed) < f.failures);

        ensure!(!should_fail, "Pruning failed");

        // Remove the data
        let mut data = self.data.lock().unwrap();
        let mut pruned_count = 0;
        for checkpoint in from..to_exclusive {
            if data.remove(&checkpoint).is_some() {
                pruned_count += 1;
            }
        }

        Ok(pruned_count)
    }

    /// Helper to configure connection failure simulation
    pub fn with_connection_failures(self, attempts: usize) -> Self {
        self.connection_failure
            .lock()
            .unwrap()
            .connection_failure_attempts = attempts;
        self
    }

    /// Helper to configure transaction failure simulation
    pub fn with_transaction_failures(mut self, failures: usize) -> Self {
        self.transaction_failures = Arc::new(Failures {
            failures,
            attempts: AtomicUsize::new(0),
        });
        self
    }

    /// Helper to configure commit watermark failure simulation
    pub fn with_commit_watermark_failures(mut self, failures: usize) -> Self {
        self.commit_watermark_failures = Arc::new(Failures {
            failures,
            attempts: AtomicUsize::new(0),
        });
        self
    }

    /// Create a new MockStore with commit delay
    pub fn with_commit_delay(mut self, delay_ms: u64) -> Self {
        self.commit_delay_ms = delay_ms;
        self
    }

    /// Helper to configure reader watermark failure simulation
    pub fn with_reader_watermark_failures(self, attempts: usize) -> Self {
        *self.set_reader_watermark_failure_attempts.lock().unwrap() = attempts;
        self
    }

    /// Helper to configure prune failure simulation for a specific range
    pub fn with_prune_failures(self, from: u64, to_exclusive: u64, failures: usize) -> Self {
        self.prune_failure_attempts.insert(
            (from, to_exclusive),
            Failures {
                failures,
                attempts: AtomicUsize::new(0),
            },
        );
        self
    }

    /// Helper to configure commit failure simulation
    pub fn with_commit_failures(mut self, failures: usize) -> Self {
        self.commit_failures = Arc::new(Failures {
            failures,
            attempts: AtomicUsize::new(0),
        });
        self
    }

    /// Get the sequential checkpoint data
    pub fn get_sequential_data(&self) -> Vec<u64> {
        self.sequential_checkpoint_data.lock().unwrap().clone()
    }

    /// Helper to get the current watermark state for testing.
    pub fn watermark(&self) -> Option<MockWatermark> {
        self.watermark.lock().unwrap().clone()
    }

    /// Helper to get the number of connection attempts for testing
    pub fn get_connection_attempts(&self) -> usize {
        self.connection_failure.lock().unwrap().connection_attempts
    }

    /// Helper to wait for a specific number of connection attempts with timeout
    pub async fn wait_for_connection_attempts(&self, expected: usize, timeout: Duration) {
        tokio::time::timeout(timeout, async {
            loop {
                if self.get_connection_attempts() >= expected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    /// Helper to get the number of prune attempts for a specific range
    pub fn prune_attempts(&self, from: u64, to_exclusive: u64) -> usize {
        self.prune_failure_attempts
            .get(&(from, to_exclusive))
            .map(|failures| failures.attempts.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Helper to wait for a specific number of prune attempts with timeout
    pub async fn wait_for_prune_attempts(
        &self,
        from: u64,
        to_exclusive: u64,
        expected: usize,
        timeout: Duration,
    ) {
        tokio::time::timeout(timeout, async {
            loop {
                if self.prune_attempts(from, to_exclusive) >= expected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    /// Wait for any data to be processed and stored, panicking if timeout is reached
    pub async fn wait_for_any_data(&self, timeout_duration: Duration) {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout_duration {
            {
                let data = self.data.lock().unwrap();
                if !data.is_empty() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Timeout waiting for any data to be processed - pipeline may be stuck");
    }

    /// Wait for data from a specific checkpoint, panicking if timeout is reached
    pub async fn wait_for_data(&self, checkpoint: u64, timeout_duration: Duration) -> Vec<u64> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout_duration {
            {
                let data = self.data.lock().unwrap();
                if let Some(values) = data.get(&checkpoint) {
                    return values.clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Timeout waiting for data for checkpoint {}", checkpoint);
    }

    /// Wait for watermark to reach the expected checkpoint, returning the watermark when reached
    pub async fn wait_for_watermark(
        &self,
        checkpoint: u64,
        timeout_duration: Duration,
    ) -> MockWatermark {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout_duration {
            if let Some(watermark) = self.watermark() {
                if watermark.checkpoint_hi_inclusive >= checkpoint {
                    return watermark;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Timeout waiting for watermark to reach {}", checkpoint);
    }
}
