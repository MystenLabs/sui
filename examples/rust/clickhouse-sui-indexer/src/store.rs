// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clickhouse::{Client, Row};
use scoped_futures::ScopedBoxFuture;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use sui_indexer_alt_framework_store_traits::{
    Connection, CommitterWatermark, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};
use url::Url;

#[derive(Clone)]
pub struct ClickHouseStore {
    client: Client,
}

pub struct ClickHouseConnection {
    pub client: Client,
}

impl ClickHouseStore {
    pub fn new(url: Url) -> Self {
        let client = Client::default()
            .with_url(url.as_str())
            .with_compression(clickhouse::Compression::Lz4);
        Self { client }
    }

    /// Create tables if they don't exist
    pub async fn create_tables_if_not_exists(&self) -> Result<()> {
        // Create watermarks table for pipeline state management
        self.client
            .query(
                "
                CREATE TABLE IF NOT EXISTS watermarks
                (
                    pipeline_name String,
                    epoch_hi_inclusive UInt64,
                    checkpoint_hi_inclusive UInt64,
                    tx_hi UInt64,
                    timestamp_ms_hi_inclusive UInt64,
                    reader_lo UInt64,
                    pruner_hi UInt64,
                    pruner_timestamp DateTime64(3, 'UTC')
                )
                ENGINE = ReplacingMergeTree(pruner_timestamp)
                ORDER BY pipeline_name
                ",
            )
            .execute()
            .await?;

        // Create transactions table for the actual indexing data
        self.client
            .query(
                "
                CREATE TABLE IF NOT EXISTS transactions
                (
                    checkpoint_sequence_number UInt64,
                    transaction_digest String,
                    indexed_at DateTime64(3, 'UTC') DEFAULT now()
                )
                ENGINE = MergeTree()
                ORDER BY (checkpoint_sequence_number, transaction_digest)
                PARTITION BY toYYYYMM(indexed_at)
                ",
            )
            .execute()
            .await?;

        Ok(())
    }
}

#[async_trait]
impl Store for ClickHouseStore {
    type Connection<'c> = ClickHouseConnection;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>> {
        Ok(ClickHouseConnection {
            client: self.client.clone(),
        })
    }
}

#[async_trait]
impl TransactionalStore for ClickHouseStore {
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

/// Row structure for watermark table operations
#[derive(Row, Serialize, Deserialize, Debug, Default)]
struct WatermarkRow {
    pipeline_name: String,
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: u64,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
    reader_lo: u64,
    pruner_hi: u64,
    pruner_timestamp: Option<DateTime<Utc>>,
}

/// Row structure for inserting watermarks
#[derive(Row, Serialize)]
struct WatermarkInsert {
    pipeline_name: String,
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: u64,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
    reader_lo: u64,
    pruner_hi: u64,
    pruner_timestamp: DateTime<Utc>,
}

#[async_trait]
impl Connection for ClickHouseConnection {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> Result<Option<CommitterWatermark>> {
        let mut cursor = self
            .client
            .query(
                "SELECT epoch_hi_inclusive, checkpoint_hi_inclusive, tx_hi, timestamp_ms_hi_inclusive 
                 FROM watermarks 
                 WHERE pipeline_name = ? 
                 ORDER BY pruner_timestamp DESC 
                 LIMIT 1"
            )
            .bind(pipeline)
            .fetch::<WatermarkRow>()?;

        let row: Option<WatermarkRow> = cursor.next().await?;
        Ok(row.map(|r| CommitterWatermark {
            epoch_hi_inclusive: r.epoch_hi_inclusive,
            checkpoint_hi_inclusive: r.checkpoint_hi_inclusive,
            tx_hi: r.tx_hi,
            timestamp_ms_hi_inclusive: r.timestamp_ms_hi_inclusive,
        }))
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> Result<Option<ReaderWatermark>> {
        let mut cursor = self
            .client
            .query(
                "SELECT checkpoint_hi_inclusive, reader_lo 
                 FROM watermarks 
                 WHERE pipeline_name = ? 
                 ORDER BY pruner_timestamp DESC 
                 LIMIT 1"
            )
            .bind(pipeline)
            .fetch::<WatermarkRow>()?;

        let row: Option<WatermarkRow> = cursor.next().await?;
        Ok(row.map(|r| ReaderWatermark {
            checkpoint_hi_inclusive: r.checkpoint_hi_inclusive,
            reader_lo: r.reader_lo,
        }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        let mut cursor = self
            .client
            .query(
                "SELECT reader_lo, pruner_hi, pruner_timestamp 
                 FROM watermarks 
                 WHERE pipeline_name = ? 
                 ORDER BY pruner_timestamp DESC 
                 LIMIT 1"
            )
            .bind(pipeline)
            .fetch::<WatermarkRow>()?;

        let row: Option<WatermarkRow> = cursor.next().await?;
        Ok(row.and_then(|r| {
            r.pruner_timestamp.map(|timestamp| {
                let now = Utc::now();
                let safe_time = timestamp + chrono::Duration::from_std(delay).unwrap_or_default();
                let wait_for_ms = (safe_time - now).num_milliseconds();

                PrunerWatermark {
                    wait_for_ms,
                    reader_lo: r.reader_lo,
                    pruner_hi: r.pruner_hi,
                }
            })
        }))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        let mut inserter = self.client.inserter("watermarks")?;
        inserter
            .write(&WatermarkInsert {
                pipeline_name: pipeline.to_string(),
                epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                tx_hi: watermark.tx_hi,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
                reader_lo: 0, // Will be updated by reader
                pruner_hi: 0, // Will be updated by pruner
                pruner_timestamp: Utc::now(),
            })?;
        inserter.end().await?;
        Ok(true)
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> Result<bool> {
        // Get current watermark first
        let current_watermark = self.committer_watermark(pipeline).await?;
        
        if let Some(current) = current_watermark {
            let mut inserter = self.client.inserter("watermarks")?;
            inserter
                .write(&WatermarkInsert {
                    pipeline_name: pipeline.to_string(),
                    epoch_hi_inclusive: current.epoch_hi_inclusive,
                    checkpoint_hi_inclusive: current.checkpoint_hi_inclusive,
                    tx_hi: current.tx_hi,
                    timestamp_ms_hi_inclusive: current.timestamp_ms_hi_inclusive,
                    reader_lo,
                    pruner_hi: 0, // Will be updated by pruner
                    pruner_timestamp: Utc::now(),
                })?;
            inserter.end().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> Result<bool> {
        // Get current watermark first
        let current_watermark = self.committer_watermark(pipeline).await?;
        let current_reader = self.reader_watermark(pipeline).await?;
        
        if let (Some(current), Some(reader)) = (current_watermark, current_reader) {
            let mut inserter = self.client.inserter("watermarks")?;
            inserter
                .write(&WatermarkInsert {
                    pipeline_name: pipeline.to_string(),
                    epoch_hi_inclusive: current.epoch_hi_inclusive,
                    checkpoint_hi_inclusive: current.checkpoint_hi_inclusive,
                    tx_hi: current.tx_hi,
                    timestamp_ms_hi_inclusive: current.timestamp_ms_hi_inclusive,
                    reader_lo: reader.reader_lo,
                    pruner_hi,
                    pruner_timestamp: Utc::now(),
                })?;
            inserter.end().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}