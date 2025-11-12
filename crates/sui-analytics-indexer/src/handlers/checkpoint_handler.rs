// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::traits::EncodeDecodeBase64;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::parquet::ParquetBatch;
use crate::tables::CheckpointEntry;
use crate::{FileType, PipelineConfig};

pub struct CheckpointBatch {
    pub inner: ParquetBatch<CheckpointEntry>,
}

impl Default for CheckpointBatch {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(FileType::Checkpoint, 0)
                .expect("Failed to create ParquetBatch"),
        }
    }
}

pub struct CheckpointHandler {
    config: PipelineConfig,
}

impl CheckpointHandler {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Processor for CheckpointHandler {
    const NAME: &'static str = "checkpoint";
    const FANOUT: usize = 10;
    type Value = CheckpointEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let checkpoint_entry = process_checkpoint(checkpoint);
        Ok(vec![checkpoint_entry])
    }
}

#[async_trait]
impl Handler for CheckpointHandler {
    type Store = ObjectStore;
    type Batch = CheckpointBatch;


    fn min_eager_rows(&self) -> usize {
        self.config.max_row_count
    }

    fn max_pending_rows(&self) -> usize {
        self.config.max_row_count * 5
    }

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        // Get first value to extract epoch and checkpoint
        let Some(first) = values.next() else {
            return BatchStatus::Pending;
        };

        batch.inner.set_epoch(first.epoch);
        batch.inner.update_last_checkpoint(first.sequence_number);

        // Write first value and remaining values
        if let Err(e) = batch
            .inner
            .write_rows(std::iter::once(first).chain(values.by_ref()))
        {
            tracing::error!("Failed to write rows to ParquetBatch: {}", e);
            return BatchStatus::Pending;
        }

        // Let framework decide when to flush based on min_eager_rows()
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let Some(file_path) = batch.inner.current_file_path() else {
            return Ok(0);
        };

        let row_count = batch.inner.row_count()?;
        let file_bytes = tokio::fs::read(file_path).await?;
        let object_path = batch.inner.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}

fn process_checkpoint(checkpoint: &Checkpoint) -> CheckpointEntry {
    let epoch = checkpoint.summary.data().epoch;
    let sequence_number = checkpoint.summary.data().sequence_number;
    let network_total_transactions = checkpoint.summary.data().network_total_transactions;
    let previous_digest = checkpoint.summary.data().previous_digest;
    let epoch_rolling_gas_cost_summary = &checkpoint.summary.data().epoch_rolling_gas_cost_summary;
    let timestamp_ms = checkpoint.summary.data().timestamp_ms;
    let end_of_epoch_data = &checkpoint.summary.data().end_of_epoch_data;

    let total_gas_cost = epoch_rolling_gas_cost_summary.computation_cost as i64
        + epoch_rolling_gas_cost_summary.storage_cost as i64
        - epoch_rolling_gas_cost_summary.storage_rebate as i64;
    let total_transaction_blocks = checkpoint.transactions.len() as u64;
    let mut total_transactions: u64 = 0;
    let mut total_successful_transaction_blocks: u64 = 0;
    let mut total_successful_transactions: u64 = 0;

    for checkpoint_transaction in &checkpoint.transactions {
        let cmds = checkpoint_transaction.transaction.kind().num_commands() as u64;
        total_transactions += cmds;
        if checkpoint_transaction.effects.status().is_ok() {
            total_successful_transaction_blocks += 1;
            total_successful_transactions += cmds;
        }
    }

    CheckpointEntry {
        sequence_number,
        checkpoint_digest: checkpoint.summary.digest().base58_encode(),
        previous_checkpoint_digest: previous_digest.map(|d| d.base58_encode()),
        epoch,
        end_of_epoch: end_of_epoch_data.is_some(),
        total_gas_cost,
        computation_cost: epoch_rolling_gas_cost_summary.computation_cost,
        storage_cost: epoch_rolling_gas_cost_summary.storage_cost,
        storage_rebate: epoch_rolling_gas_cost_summary.storage_rebate,
        non_refundable_storage_fee: epoch_rolling_gas_cost_summary.non_refundable_storage_fee,
        total_transaction_blocks,
        total_transactions,
        total_successful_transaction_blocks,
        total_successful_transactions,
        network_total_transaction: network_total_transactions,
        timestamp_ms,
        validator_signature: checkpoint.summary.auth_sig().signature.encode_base64(),
    }
}
