// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::ensure;
use async_trait::async_trait;
use dashmap::DashMap;
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework_store_traits::testing::mock_store::MockConnection;
use sui_indexer_alt_framework_store_traits::testing::mock_store::MockStore;
use sui_indexer_alt_framework_store_traits::testing::mock_store::MockWatermark;

use crate::store::CommitterWatermark;
use crate::store::ConcurrentConnection;
use crate::store::ConcurrentStore;
use crate::store::Connection;
use crate::store::InitWatermark;
use crate::store::PrunerWatermark;
use crate::store::ReaderWatermark;
use crate::store::SequentialConnection;
use crate::store::SequentialStore;
use crate::store::Store;

/// Configuration for simulating connection failures in tests.
#[derive(Default)]
pub struct ConnectionFailure {
    /// Number of failures before connection succeeds.
    pub connection_failure_attempts: usize,
    /// Delay in milliseconds for each connection attempt.
    pub connection_delay_ms: u64,
    /// Counter for tracking total connection attempts.
    pub connection_attempts: usize,
}

/// Configuration for simulating failures in tests.
#[derive(Default)]
pub struct Failures {
    /// Number of failures to simulate before allowing success.
    pub failures: usize,
    /// Counter for tracking total attempts.
    pub attempts: AtomicUsize,
}

/// Fallible in-memory `Store`/`Connection` for testing framework mechanics. Operators should
/// largely test their real store implementations.
#[derive(Clone, Default)]
pub struct FallibleMockStore {
    /// Shared store-trait mock that owns watermark and chain-id state.
    pub delegate: MockStore,
    /// Maps each pipeline's name to a map of checkpoint sequence numbers to a vector of numbers.
    pub data: Arc<DashMap<String, DashMap<u64, Vec<u64>>>>,
    /// Tracks the order of checkpoint processing for testing sequential processing.
    pub sequential_checkpoint_data: Arc<Mutex<Vec<u64>>>,
    /// Controls pruning failure simulation for testing retry behavior.
    pub prune_failure_attempts: Arc<DashMap<(u64, u64), Failures>>,
    /// Configuration for simulating connection failures in tests.
    pub connection_failure: Arc<Mutex<ConnectionFailure>>,
    /// Number of remaining failures for set_reader_watermark operation.
    pub set_reader_watermark_failure_attempts: Arc<Mutex<usize>>,
    /// Configuration for simulating transaction failures in tests.
    pub transaction_failures: Arc<Failures>,
    /// Configuration for simulating commit failures in tests.
    pub commit_failures: Arc<Failures>,
    /// Configuration for simulating commit watermark failures in tests.
    pub commit_watermark_failures: Arc<Failures>,
    /// Delay in milliseconds for each transaction commit.
    pub commit_delay_ms: u64,
}

#[derive(Clone)]
pub struct FallibleMockConnection<'c>(pub &'c FallibleMockStore, MockConnection<'c>);

impl FallibleMockStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn commit_bulk_data(
        &self,
        pipeline: &'static str,
        values: HashMap<u64, Vec<u64>>,
    ) -> anyhow::Result<usize> {
        if self.commit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.commit_delay_ms)).await;
        }

        let prev = self
            .commit_failures
            .attempts
            .fetch_add(1, Ordering::Relaxed);
        ensure!(
            prev >= self.commit_failures.failures,
            "Transaction failed, remaining failures: {}",
            self.commit_failures.failures - prev
        );

        let key = pipeline.to_string();
        let mut total = 0;
        let inner = self.data.entry(key).or_default();
        for (cp, v) in values {
            total += v.len();
            inner.entry(cp).or_default().extend(v);
        }
        Ok(total)
    }

    pub async fn commit_data(
        &self,
        pipeline: &'static str,
        checkpoint: u64,
        values: Vec<u64>,
    ) -> anyhow::Result<usize> {
        if self.commit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.commit_delay_ms)).await;
        }

        let prev = self
            .commit_failures
            .attempts
            .fetch_add(1, Ordering::Relaxed);
        ensure!(
            prev >= self.commit_failures.failures,
            "Transaction failed, remaining failures: {}",
            self.commit_failures.failures - prev
        );

        let key = pipeline.to_string();
        let mut total = 0;
        let inner = self.data.entry(key).or_default();
        total += values.len();
        inner.insert(checkpoint, values);
        Ok(total)
    }

    pub fn prune_data(
        &self,
        pipeline: &'static str,
        from: u64,
        to_exclusive: u64,
    ) -> anyhow::Result<usize> {
        let should_fail = self
            .prune_failure_attempts
            .get(&(from, to_exclusive))
            .is_some_and(|f| f.attempts.fetch_add(1, Ordering::Relaxed) < f.failures);

        ensure!(!should_fail, "Pruning failed");

        let key = pipeline.to_string();
        let Some(pipeline_data) = self.data.get_mut(&key) else {
            return Ok(0);
        };
        let mut pruned_count = 0;
        for checkpoint in from..to_exclusive {
            if pipeline_data.remove(&checkpoint).is_some() {
                pruned_count += 1;
            }
        }

        Ok(pruned_count)
    }

    pub fn with_connection_failures(self, attempts: usize) -> Self {
        self.connection_failure
            .lock()
            .unwrap()
            .connection_failure_attempts = attempts;
        self
    }

    pub fn with_transaction_failures(mut self, failures: usize) -> Self {
        self.transaction_failures = Arc::new(Failures {
            failures,
            attempts: AtomicUsize::new(0),
        });
        self
    }

    pub fn with_commit_watermark_failures(mut self, failures: usize) -> Self {
        self.commit_watermark_failures = Arc::new(Failures {
            failures,
            attempts: AtomicUsize::new(0),
        });
        self
    }

    pub fn with_commit_delay(mut self, delay_ms: u64) -> Self {
        self.commit_delay_ms = delay_ms;
        self
    }

    pub fn with_reader_watermark_failures(self, attempts: usize) -> Self {
        *self.set_reader_watermark_failure_attempts.lock().unwrap() = attempts;
        self
    }

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

    pub fn with_commit_failures(mut self, failures: usize) -> Self {
        self.commit_failures = Arc::new(Failures {
            failures,
            attempts: AtomicUsize::new(0),
        });
        self
    }

    pub fn with_watermark(self, pipeline_task: &str, watermark: MockWatermark) -> Self {
        self.delegate
            .watermarks
            .insert(pipeline_task.to_string(), watermark);
        self
    }

    pub fn with_data(self, pipeline_task: &str, data: HashMap<u64, Vec<u64>>) -> Self {
        self.data
            .insert(pipeline_task.to_string(), DashMap::from_iter(data));
        self
    }

    pub fn watermark(&self, pipeline_task: &str) -> Option<MockWatermark> {
        self.delegate
            .watermarks
            .get(pipeline_task)
            .map(|w| w.clone())
    }

    pub fn get_sequential_data(&self) -> Vec<u64> {
        self.sequential_checkpoint_data.lock().unwrap().clone()
    }

    pub fn get_connection_attempts(&self) -> usize {
        self.connection_failure.lock().unwrap().connection_attempts
    }

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

    pub fn prune_attempts(&self, from: u64, to_exclusive: u64) -> usize {
        self.prune_failure_attempts
            .get(&(from, to_exclusive))
            .map(|failures| failures.attempts.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

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

    pub async fn wait_for_any_data(&self, pipeline_task: &str, timeout_duration: Duration) {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout_duration {
            if self.data.contains_key(pipeline_task) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Timeout waiting for any data to be processed - pipeline may be stuck");
    }

    pub async fn wait_for_data(
        &self,
        pipeline_task: &str,
        checkpoint: u64,
        timeout_duration: Duration,
    ) -> Vec<u64> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout_duration {
            if let Some(pipeline_data) = self.data.get(pipeline_task)
                && let Some(values) = pipeline_data.get(&checkpoint)
            {
                return values.clone();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Timeout waiting for data for checkpoint {}", checkpoint);
    }

    pub async fn wait_for_watermark(
        &self,
        pipeline_task: &str,
        checkpoint: u64,
        timeout_duration: Duration,
    ) -> MockWatermark {
        let start = tokio::time::Instant::now();
        while start.elapsed() < timeout_duration {
            if let Some(watermark) = self.watermark(pipeline_task)
                && watermark
                    .checkpoint_hi_inclusive
                    .is_some_and(|c| c >= checkpoint)
            {
                return watermark;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "Timeout waiting for watermark on pipeline {} to reach {}",
            pipeline_task, checkpoint
        );
    }
}

#[async_trait]
impl Connection for FallibleMockConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<InitWatermark>> {
        self.1
            .init_watermark(pipeline_task, checkpoint_hi_inclusive)
            .await
    }

    async fn accepts_chain_id(
        &mut self,
        pipeline_task: &str,
        chain_id: [u8; 32],
    ) -> anyhow::Result<bool> {
        self.1.accepts_chain_id(pipeline_task, chain_id).await
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>, anyhow::Error> {
        self.1.committer_watermark(pipeline_task).await
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
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

        self.1
            .set_committer_watermark(pipeline_task, watermark)
            .await
    }
}

#[async_trait]
impl ConcurrentConnection for FallibleMockConnection<'_> {
    async fn reader_watermark(
        &mut self,
        pipeline: &str,
    ) -> Result<Option<ReaderWatermark>, anyhow::Error> {
        self.1.reader_watermark(pipeline).await
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>, anyhow::Error> {
        self.1.pruner_watermark(pipeline, delay).await
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
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

        self.1.set_reader_watermark(pipeline, reader_lo).await
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        self.1.set_pruner_watermark(pipeline, pruner_hi).await
    }
}

#[async_trait]
impl SequentialConnection for FallibleMockConnection<'_> {}

#[async_trait]
impl ConcurrentStore for FallibleMockStore {
    type ConcurrentConnection<'c> = FallibleMockConnection<'c>;
}

#[async_trait]
impl Store for FallibleMockStore {
    type Connection<'c> = FallibleMockConnection<'c>;

    async fn connect(&self) -> anyhow::Result<Self::Connection<'_>> {
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

        let delegate = self.delegate.connect().await?;
        Ok(FallibleMockConnection(self, delegate))
    }
}

#[async_trait]
impl SequentialStore for FallibleMockStore {
    type SequentialConnection<'c> = FallibleMockConnection<'c>;

    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let prev = self
            .transaction_failures
            .attempts
            .fetch_add(1, Ordering::Relaxed);
        ensure!(
            prev >= self.transaction_failures.failures,
            "Transaction failed, remaining failures: {}",
            self.transaction_failures.failures - prev
        );

        let snapshot: HashMap<String, MockWatermark> = self
            .delegate
            .watermarks
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();

        let mut conn = self.connect().await?;
        match f(&mut conn).await {
            Ok(r) => Ok(r),
            Err(e) => {
                self.delegate.watermarks.clear();
                for (k, v) in snapshot {
                    self.delegate.watermarks.insert(k, v);
                }
                Err(e)
            }
        }
    }
}
