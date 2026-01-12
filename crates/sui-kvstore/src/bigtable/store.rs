// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BigTable Store implementation for sui-indexer-alt-framework.
//!
//! This implements the `Store` and `Connection` traits to allow the new framework
//! to use BigTable for watermark storage. The watermark is stored in the same format
//! as the legacy binary (key=[0] in watermark_alt table) to allow both binaries to
//! run in parallel during migration.
//!
//! Note: The legacy binary stores "next checkpoint to process", while the new framework
//! uses "checkpoint_hi_inclusive" (last processed). We translate between these with +1/-1.

use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use sui_indexer_alt_framework_store_traits::{
    CommitterWatermark, Connection, PrunerWatermark, ReaderWatermark, Store,
};
use sui_types::full_checkpoint_content::CheckpointData;

use super::client::BigTableClient;
use crate::{KeyValueStoreReader, KeyValueStoreWriter, TransactionData};

/// A Store implementation backed by BigTable.
///
/// This store manages watermarks in the `watermark_alt` table using key `[0]`
/// for compatibility with the legacy kvstore binary.
#[derive(Clone)]
pub struct BigTableStore {
    client: BigTableClient,
}

impl BigTableStore {
    pub fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Store for BigTableStore {
    type Connection<'c> = BigTableConnection<'c>;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>> {
        Ok(BigTableConnection {
            client: self.client.clone(),
            _marker: std::marker::PhantomData,
        })
    }
}

/// A connection to BigTable for watermark operations and data writes.
pub struct BigTableConnection<'a> {
    client: BigTableClient,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl BigTableConnection<'_> {
    /// Process a checkpoint by writing all its data to BigTable.
    ///
    /// This is a copy of KvWorker::process_checkpoint to decouple the new binary
    /// from the legacy code path. Changes here won't affect the legacy binary.
    pub async fn process_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()> {
        let mut objects = vec![];
        let mut transactions = vec![];
        for transaction in &checkpoint.transactions {
            let full_transaction = TransactionData {
                transaction: transaction.transaction.clone(),
                effects: transaction.effects.clone(),
                events: transaction.events.clone(),
                checkpoint_number: checkpoint.checkpoint_summary.sequence_number,
                timestamp: checkpoint.checkpoint_summary.timestamp_ms,
            };
            for object in &transaction.output_objects {
                objects.push(object);
            }
            transactions.push(full_transaction);
        }
        self.client
            .save_objects(&objects, checkpoint.checkpoint_summary.timestamp_ms)
            .await?;
        self.client.save_transactions(&transactions).await?;
        self.client.save_checkpoint(checkpoint).await?;
        if let Some(epoch_info) = checkpoint.epoch_info()? {
            if epoch_info.epoch > 0 {
                let mut prev = self
                    .client
                    .get_epoch(epoch_info.epoch - 1)
                    .await?
                    .with_context(|| {
                        format!(
                            "previous epoch {} not found when processing epoch {}",
                            epoch_info.epoch - 1,
                            epoch_info.epoch
                        )
                    })?;
                prev.end_checkpoint = epoch_info.start_checkpoint.map(|sq| sq - 1);
                prev.end_timestamp_ms = epoch_info.start_timestamp_ms;
                self.client.save_epoch(prev).await?;
            }
            self.client.save_epoch(epoch_info).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Connection for BigTableConnection<'_> {
    async fn init_watermark(
        &mut self,
        _pipeline_task: &str,
        _default_next_checkpoint: u64,
    ) -> Result<Option<u64>> {
        // Read existing watermark from BigTable.
        // The stored value is "next checkpoint to process", so we subtract 1
        // to get checkpoint_hi_inclusive (last processed checkpoint).
        //
        // Phase 1: We ignore pipeline_task and use the shared key [0] for
        // compatibility with the legacy binary.
        let next_checkpoint = self.client.get_latest_checkpoint().await?;
        if next_checkpoint == 0 {
            // No watermark exists yet - return None to indicate initialization.
            // We don't actually write the default because it would be incompatible
            // with the legacy binary.
            Ok(None)
        } else {
            // Return the last processed checkpoint (next - 1)
            Ok(Some(next_checkpoint - 1))
        }
    }

    async fn committer_watermark(
        &mut self,
        _pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>> {
        let next_checkpoint = self.client.get_latest_checkpoint().await?;
        if next_checkpoint == 0 {
            Ok(None)
        } else {
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: next_checkpoint - 1,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
            }))
        }
    }

    async fn set_committer_watermark(
        &mut self,
        _pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        // Write checkpoint number to watermark_alt table.
        // The legacy binary stores "next checkpoint to process", so we add 1
        // to checkpoint_hi_inclusive.
        // Uses checkpoint timestamp for cell timestamp (deterministic).
        self.client
            .save_watermark(watermark.checkpoint_hi_inclusive + 1)
            .await?;
        Ok(true)
    }

    // Phase 1: Return Ok(None) - reader/pruner watermarks not needed for concurrent
    // pipelines without pruning.

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> Result<Option<ReaderWatermark>> {
        Ok(None)
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        Ok(None)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        _reader_lo: u64,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _pruner_hi: u64,
    ) -> Result<bool> {
        Ok(false)
    }
}
