// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use scoped_futures::ScopedBoxFuture;
use tokio::time::Duration;

use crate::store::{
    CommitterWatermark, Connection, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};

#[derive(Default)]
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
}

/// A mock store for testing. It maintains a map of checkpoint sequence numbers to transaction
/// sequence numbers, and a watermark that can be used to test the watermark task.
#[derive(Clone)]
pub struct MockStore {
    /// Tracks various watermark states (committer, reader, pruner)
    pub watermarks: Arc<Mutex<MockWatermark>>,
    /// Stores the actual data, mapping checkpoint sequence numbers to transaction sequence numbers
    pub data: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
    /// Tracks the order of checkpoint processing for testing sequential processing
    /// Each entry is the checkpoint number that was processed
    pub sequential_checkpoint_data: Arc<Mutex<Vec<u64>>>,
    /// Controls pruning failure simulation for testing retry behavior.
    /// Maps from [from_checkpoint, to_checkpoint_exclusive) to number of remaining failure attempts.
    /// When a prune operation is attempted on a range, if there are remaining failures,
    /// the operation will fail and the counter will be decremented.
    pub prune_failure_attempts: Arc<Mutex<HashMap<(u64, u64), usize>>>,
    /// Configuration for simulating connection failures in tests
    pub connection_failure: Arc<Mutex<ConnectionFailure>>,
    /// Number of remaining failures for set_reader_watermark operation
    pub set_reader_watermark_failure_attempts: Arc<Mutex<usize>>,
    /// Number of remaining failures for transaction operation
    pub transaction_failure_attempts: Arc<Mutex<usize>>,
}

impl Default for MockStore {
    fn default() -> Self {
        Self {
            watermarks: Arc::new(Mutex::new(MockWatermark::default())),
            data: Arc::new(Mutex::new(HashMap::new())),
            sequential_checkpoint_data: Arc::new(Mutex::new(Vec::new())),
            prune_failure_attempts: Arc::new(Mutex::new(HashMap::new())),
            connection_failure: Arc::new(Mutex::new(ConnectionFailure::default())),
            set_reader_watermark_failure_attempts: Arc::new(Mutex::new(0)),
            transaction_failure_attempts: Arc::new(Mutex::new(0)),
        }
    }
}

#[derive(Clone)]
pub struct MockConnection<'c>(pub &'c MockStore);

#[async_trait]
impl Connection for MockConnection<'_> {
    async fn committer_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> Result<Option<CommitterWatermark>, anyhow::Error> {
        let watermarks = self.0.watermarks.lock().unwrap();
        Ok(Some(CommitterWatermark {
            epoch_hi_inclusive: watermarks.epoch_hi_inclusive,
            checkpoint_hi_inclusive: watermarks.checkpoint_hi_inclusive,
            tx_hi: watermarks.tx_hi,
            timestamp_ms_hi_inclusive: watermarks.timestamp_ms_hi_inclusive,
        }))
    }

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> Result<Option<ReaderWatermark>, anyhow::Error> {
        let watermarks = self.0.watermarks.lock().unwrap();
        Ok(Some(ReaderWatermark {
            checkpoint_hi_inclusive: watermarks.checkpoint_hi_inclusive,
            reader_lo: watermarks.reader_lo,
        }))
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>, anyhow::Error> {
        let watermarks = self.0.watermarks.lock().unwrap();
        let elapsed_ms = watermarks.pruner_timestamp as i64
            - SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
        let wait_for_ms = delay.as_millis() as i64 + elapsed_ms;
        Ok(Some(PrunerWatermark {
            pruner_hi: watermarks.pruner_hi,
            reader_lo: watermarks.reader_lo,
            wait_for_ms,
        }))
    }

    async fn set_committer_watermark(
        &mut self,
        _pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        watermarks.epoch_hi_inclusive = watermark.epoch_hi_inclusive;
        watermarks.checkpoint_hi_inclusive = watermark.checkpoint_hi_inclusive;
        watermarks.tx_hi = watermark.tx_hi;
        watermarks.timestamp_ms_hi_inclusive = watermark.timestamp_ms_hi_inclusive;
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

        let mut watermarks = self.0.watermarks.lock().unwrap();
        watermarks.reader_lo = reader_lo;
        watermarks.pruner_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        watermarks.pruner_hi = pruner_hi;
        Ok(true)
    }
}

#[async_trait]
impl Store for MockStore {
    type Connection<'c> = MockConnection<'c>;

    async fn connect(&self) -> anyhow::Result<Self::Connection<'_>> {
        // Check for connection failure simulation
        let should_fail = {
            let mut failure = self.connection_failure.lock().unwrap();
            if failure.connection_failure_attempts > 0 {
                failure.connection_failure_attempts -= 1;
                true
            } else {
                false
            }
        };
        let delay_ms = {
            let failure = self.connection_failure.lock().unwrap();
            failure.connection_delay_ms
        };

        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        if should_fail {
            return Err(anyhow::anyhow!("Connection failed"));
        }

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
        {
            let mut remaining = self.transaction_failure_attempts.lock().unwrap();
            if *remaining > 0 {
                *remaining -= 1;
                return Err(anyhow::anyhow!(
                    "Transaction failed, remaining failures: {}",
                    *remaining
                ));
            }
        }

        let mut conn = self.connect().await?;
        f(&mut conn).await
    }
}

impl MockStore {
    /// Get the sequential checkpoint data for testing.
    /// This returns a copy of the data to avoid holding the lock.
    pub fn get_sequential_data(&self) -> Vec<u64> {
        self.sequential_checkpoint_data.lock().unwrap().clone()
    }
}
