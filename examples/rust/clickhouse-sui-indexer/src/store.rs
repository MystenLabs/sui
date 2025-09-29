// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use clickhouse::{Client, Row};
use scoped_futures::ScopedBoxFuture;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use sui_indexer_alt_framework_store_traits::{
    CommitterWatermark, Connection, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};
use url::Url;

#[derive(Clone)]
pub struct ClickHouseStore {
    client: Client,
}

pub struct ClickHouseConnection {
    pub client: Client,
}

/// Row structure for watermark table operations
#[derive(Row, Serialize, Deserialize, Debug, Default)]
struct WatermarkRow {
    pipeline: String,
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: u64,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
    reader_lo: u64,
    pruner_hi: u64,
    pruner_timestamp: u64, // Unix timestamp in milliseconds
}

impl ClickHouseStore {
    pub fn new(url: Url) -> Self {
        let client = Client::default()
            .with_url(url.as_str())
            .with_user("dev") // Simple user for local development
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
                    pipeline String,
                    epoch_hi_inclusive UInt64,
                    checkpoint_hi_inclusive UInt64,
                    tx_hi UInt64,
                    timestamp_ms_hi_inclusive UInt64,
                    reader_lo UInt64,
                    pruner_hi UInt64,
                    pruner_timestamp UInt64
                )
                ENGINE = MergeTree()
                ORDER BY pipeline
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
                ORDER BY checkpoint_sequence_number
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
                 WHERE pipeline = ? 
                 ORDER BY pruner_timestamp DESC 
                 LIMIT 1"
            )
            .bind(pipeline)
            .fetch::<(u64, u64, u64, u64)>()?;

        let row: Option<(u64, u64, u64, u64)> = cursor.next().await?;
        Ok(row.map(
            |(epoch_hi, checkpoint_hi, tx_hi, timestamp_hi)| CommitterWatermark {
                epoch_hi_inclusive: epoch_hi,
                checkpoint_hi_inclusive: checkpoint_hi,
                tx_hi,
                timestamp_ms_hi_inclusive: timestamp_hi,
            },
        ))
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
                 WHERE pipeline = ? 
                 ORDER BY pruner_timestamp DESC 
                 LIMIT 1",
            )
            .bind(pipeline)
            .fetch::<(u64, u64)>()?;

        let row: Option<(u64, u64)> = cursor.next().await?;
        Ok(row.map(|(checkpoint_hi, reader_lo)| ReaderWatermark {
            checkpoint_hi_inclusive: checkpoint_hi,
            reader_lo,
        }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        // Follow PostgreSQL pattern: calculate wait_for_ms on database side
        // We do this so that we can rely on the database to keep a consistent sense of time.
        // Using own clocks can potentially be subject to some clock skew.
        let delay_ms = delay.as_millis() as i64;
        let mut cursor = self
            .client
            .query(
                "SELECT reader_lo, pruner_hi, 
                        toInt64(? + (pruner_timestamp - toUnixTimestamp64Milli(now64()))) as wait_for_ms
                 FROM watermarks 
                 WHERE pipeline = ? 
                 ORDER BY pruner_timestamp DESC 
                 LIMIT 1"
            )
            .bind(delay_ms)
            .bind(pipeline)
            .fetch::<(u64, u64, i64)>()?;

        let row: Option<(u64, u64, i64)> = cursor.next().await?;
        Ok(
            row.map(|(reader_lo, pruner_hi, wait_for_ms)| PrunerWatermark {
                wait_for_ms,
                reader_lo,
                pruner_hi,
            }),
        )
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        // Follow PostgreSQL pattern: check if row exists, then UPDATE or INSERT accordingly

        // First check if pipeline exists and get current checkpoint
        let mut cursor = self
            .client
            .query("SELECT checkpoint_hi_inclusive FROM watermarks WHERE pipeline = ? LIMIT 1")
            .bind(pipeline)
            .fetch::<u64>()?;

        let existing_checkpoint: Option<u64> = cursor.next().await?;

        if let Some(existing_checkpoint) = existing_checkpoint {
            // Row exists - only update if checkpoint advances
            if existing_checkpoint < watermark.checkpoint_hi_inclusive {
                self.client
                    .query(
                        "ALTER TABLE watermarks 
                         UPDATE 
                             epoch_hi_inclusive = ?,
                             checkpoint_hi_inclusive = ?,
                             tx_hi = ?,
                             timestamp_ms_hi_inclusive = ?
                         WHERE pipeline = ?",
                    )
                    .bind(watermark.epoch_hi_inclusive)
                    .bind(watermark.checkpoint_hi_inclusive)
                    .bind(watermark.tx_hi)
                    .bind(watermark.timestamp_ms_hi_inclusive)
                    .bind(pipeline)
                    .execute()
                    .await?;
            }
        } else {
            // No existing row - insert new one
            let mut inserter = self.client.inserter("watermarks")?;
            inserter.write(&WatermarkRow {
                pipeline: pipeline.to_string(),
                epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                tx_hi: watermark.tx_hi,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
                reader_lo: 0, // Will be updated by reader
                pruner_hi: 0, // Will be updated by pruner
                pruner_timestamp: Utc::now().timestamp_millis() as u64,
            })?;
            inserter.end().await?;
        }

        Ok(true)
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> Result<bool> {
        // Follow PostgreSQL pattern: simple UPDATE with timestamp update and advancement check
        self.client
            .query(
                "ALTER TABLE watermarks 
                 UPDATE reader_lo = ?, pruner_timestamp = toUnixTimestamp64Milli(now64())
                 WHERE pipeline = ? AND reader_lo < ?",
            )
            .bind(reader_lo)
            .bind(pipeline)
            .bind(reader_lo)
            .execute()
            .await?;

        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> Result<bool> {
        // Follow PostgreSQL pattern: simple UPDATE statement
        self.client
            .query(
                "ALTER TABLE watermarks 
                 UPDATE pruner_hi = ? 
                 WHERE pipeline = ?",
            )
            .bind(pruner_hi)
            .bind(pipeline)
            .execute()
            .await?;

        Ok(true)
    }
}
