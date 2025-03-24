// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::Utc;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
    sync::{Arc, RwLock},
    time::Duration,
};
use sui_indexer_alt_metrics::stats::{DbConnectionStats, DbConnectionStatsSnapshot};

use crate::store::{DbConnection, Store};

pub struct InMemoryWatermark {
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
    pub reader_lo: i64,
    pub pruner_timestamp: i64,
    pub pruner_hi: i64,
}

/// A simple in-memory store implementation for testing
#[derive(Clone, Default)]
pub struct InMemoryStore {
    // Main data storage: table_name -> (row_id -> row_data)
    data: Arc<RwLock<HashMap<String, HashMap<String, Vec<u8>>>>>,

    // Watermarks: pipeline_name -> InMemoryWatermark
    watermarks: Arc<RwLock<HashMap<String, InMemoryWatermark>>>,

    // Simple metrics
    connection_count: Arc<AtomicUsize>,
}

/// Connection to the in-memory store
pub struct InMemoryConnection<'a> {
    store: &'a InMemoryStore,
}

impl<'c> DbConnection for InMemoryConnection<'c> {}

impl InMemoryStore {
    /// Create a new empty in-memory store
    pub fn new() -> Self {
        Self::default()
    }

    /// Helper to directly insert data for testing
    #[cfg(test)]
    pub fn insert(&self, table: &str, key: &str, value: Vec<u8>) {
        let mut data = self.data.write().unwrap();
        data.entry(table.to_string())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value);
    }

    /// Helper to directly get data for testing
    #[cfg(test)]
    pub fn get(&self, table: &str, key: &str) -> Option<Vec<u8>> {
        self.data
            .read()
            .unwrap()
            .get(table)
            .and_then(|table_data| table_data.get(key).cloned())
    }

    /// Set a watermark for testing
    #[cfg(test)]
    pub fn set_watermark(&self, pipeline: &str, watermark: InMemoryWatermark) {
        self.watermarks
            .write()
            .unwrap()
            .insert(pipeline.to_string(), watermark);
    }
}

#[async_trait]
impl Store for InMemoryStore {
    type Connection<'c>
        = InMemoryConnection<'c>
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        self.connection_count.fetch_add(1, Ordering::SeqCst);
        Ok(InMemoryConnection { store: self })
    }

    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64, i64, i64)>> {
        let watermarks = self.watermarks.read().unwrap();
        if let Some(watermark) = watermarks.get(pipeline) {
            Ok(Some((
                watermark.epoch_hi_inclusive,
                watermark.checkpoint_hi_inclusive,
                watermark.tx_hi,
                watermark.timestamp_ms_hi_inclusive,
            )))
        } else {
            Ok(None)
        }
    }

    async fn update_committer_watermark(
        &self,
        pipeline: &'static str,
        epoch_hi_inclusive: i64,
        checkpoint_hi_inclusive: i64,
        tx_hi: i64,
        timestamp_ms_hi_inclusive: i64,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.watermarks.write().unwrap();
        let entry = watermarks
            .entry(pipeline.to_string())
            .or_insert(InMemoryWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 0,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                reader_lo: 0,
                pruner_timestamp: 0,
                pruner_hi: 0,
            });

        // Only update if the new checkpoint is higher
        if checkpoint_hi_inclusive > entry.checkpoint_hi_inclusive {
            entry.checkpoint_hi_inclusive = checkpoint_hi_inclusive;
            entry.tx_hi = tx_hi;
            entry.timestamp_ms_hi_inclusive = timestamp_ms_hi_inclusive;
            entry.epoch_hi_inclusive = epoch_hi_inclusive;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_reader_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64)>> {
        let watermarks = self.watermarks.read().unwrap();
        if let Some(watermark) = watermarks.get(pipeline) {
            Ok(Some((
                watermark.checkpoint_hi_inclusive,
                watermark.reader_lo,
            )))
        } else {
            Ok(None)
        }
    }

    async fn update_reader_watermark(
        &self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.watermarks.write().unwrap();
        let entry = watermarks
            .entry(pipeline.to_string())
            .or_insert(InMemoryWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 0,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                reader_lo: 0,
                pruner_timestamp: 0,
                pruner_hi: 0,
            });

        if reader_lo > entry.reader_lo {
            entry.reader_lo = reader_lo;
            entry.pruner_timestamp = Utc::now().timestamp_millis();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_pruner_watermark(
        &self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<(i64, i64, i64)>> {
        let watermarks = self.watermarks.read().unwrap();
        if let Some(watermark) = watermarks.get(pipeline) {
            // Calculate wait_for as specified in the trait documentation
            let current_time_ms = Utc::now().timestamp_millis();
            let wait_for_ms =
                delay.as_millis() as i64 + (watermark.pruner_timestamp - current_time_ms);

            Ok(Some((
                watermark.pruner_hi,
                watermark.reader_lo,
                wait_for_ms.max(0),
            )))
        } else {
            Ok(None)
        }
    }

    async fn update_pruner_watermark(
        &self,
        pipeline: &'static str,
        pruner_hi: i64,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.watermarks.write().unwrap();
        let entry = watermarks
            .entry(pipeline.to_string())
            .or_insert(InMemoryWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 0,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                reader_lo: 0,
                pruner_timestamp: 0,
                pruner_hi: 0,
            });

        // Only update if the new pruner_hi is higher
        if pruner_hi > entry.pruner_hi {
            entry.pruner_hi = pruner_hi;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl DbConnectionStats for InMemoryStore {
    fn get_connection_stats(&self) -> DbConnectionStatsSnapshot {
        DbConnectionStatsSnapshot {
            connections: self.connection_count.load(Ordering::SeqCst) as usize,
            idle_connections: 0,
            get_direct: 0,
            get_waited: 0,
            get_timed_out: 0,
            get_wait_time_ms: 0,
            connections_created: 0,
            connections_closed_broken: 0,
            connections_closed_invalid: 0,
            connections_closed_max_lifetime: 0,
            connections_closed_idle_timeout: 0,
        }
    }
}
