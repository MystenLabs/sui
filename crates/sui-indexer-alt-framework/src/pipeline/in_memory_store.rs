// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use scoped_futures::ScopedBoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sui_indexer_alt_framework_store::store::{
    CommitterWatermark, DbConnection, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};

#[derive(Clone, Debug)]
struct WatermarkData {
    epoch_hi_inclusive: i64,
    checkpoint_hi_inclusive: i64,
    tx_hi: i64,
    timestamp_ms_hi_inclusive: i64,
    reader_lo: i64,
    pruner_timestamp: SystemTime,
    pruner_hi: i64,
}

impl Default for WatermarkData {
    fn default() -> Self {
        Self {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 0,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
            reader_lo: 0,
            pruner_timestamp: UNIX_EPOCH,
            pruner_hi: 0,
        }
    }
}

/// A simple in-memory store for testing purposes. Contains a `watermarks` "table" and a `data`
/// table for mapping table names to a vector of strings.
#[derive(Clone)]
pub struct InMemoryStore {
    /// A map of pipeline names to their watermark data.
    watermarks: Arc<Mutex<HashMap<String, WatermarkData>>>,
    /// A map of pipeline names to a vector of strings.
    data: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            watermarks: Arc::new(Mutex::new(HashMap::new())),
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DbConnection for InMemoryStore {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let data = self.watermarks.lock().unwrap();

        if let Some(watermark) = data.get(pipeline) {
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                tx_hi: watermark.tx_hi,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
            }))
        } else {
            Ok(None)
        }
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        let data = self.watermarks.lock().unwrap();

        if let Some(watermark) = data.get(pipeline) {
            Ok(Some(ReaderWatermark {
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                reader_lo: watermark.reader_lo,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let mut data = self.watermarks.lock().unwrap();

        let entry = data.entry(pipeline.to_string()).or_default();

        // Only update if the checkpoint is higher
        if entry.checkpoint_hi_inclusive < watermark.checkpoint_hi_inclusive {
            entry.epoch_hi_inclusive = watermark.epoch_hi_inclusive;
            entry.checkpoint_hi_inclusive = watermark.checkpoint_hi_inclusive;
            entry.tx_hi = watermark.tx_hi;
            entry.timestamp_ms_hi_inclusive = watermark.timestamp_ms_hi_inclusive;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool> {
        let mut data = self.watermarks.lock().unwrap();

        if let Some(entry) = data.get_mut(pipeline) {
            if entry.reader_lo < reader_lo {
                entry.reader_lo = reader_lo;
                entry.pruner_timestamp = SystemTime::now();
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        let data = self.watermarks.lock().unwrap();

        if let Some(watermark) = data.get(pipeline) {
            let now = SystemTime::now();
            let wait_for = match now.duration_since(watermark.pruner_timestamp) {
                Ok(elapsed) => {
                    // If more time has elapsed than the delay, wait_for should be 0
                    if elapsed >= delay {
                        0
                    } else {
                        // Otherwise, calculate the remaining time
                        let remaining = delay - elapsed;
                        remaining.as_millis() as i64
                    }
                }
                // If pruner_timestamp is in the future (should not happen normally),
                // we add that duration to the delay
                Err(e) => {
                    let time_diff = e.duration();
                    (delay + time_diff).as_millis() as i64
                }
            };

            Ok(Some(PrunerWatermark {
                wait_for,
                pruner_hi: watermark.pruner_hi,
                reader_lo: watermark.reader_lo,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: i64,
    ) -> anyhow::Result<bool> {
        let mut data = self.watermarks.lock().unwrap();

        if let Some(entry) = data.get_mut(pipeline) {
            entry.pruner_hi = pruner_hi;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[async_trait]
impl Store for InMemoryStore {
    type Connection<'c> = InMemoryStore;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        Ok(InMemoryStore {
            watermarks: Arc::clone(&self.watermarks),
            data: Arc::clone(&self.data),
        })
    }
}

#[async_trait]
impl TransactionalStore for InMemoryStore {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let mut conn = self.connect().await?;
        f(&mut conn).await
    }
}
