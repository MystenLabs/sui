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

use crate::FileType;
use crate::tables::CheckpointEntry;
use crate::writers::AnalyticsWriter;

pub struct CheckpointHandler;

impl CheckpointHandler {
    pub fn new() -> Self {
        Self
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
    type Batch = Vec<CheckpointEntry>;

    const MIN_EAGER_ROWS: usize = 100_000;
    const MAX_PENDING_ROWS: usize = 500_000;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        batch.extend(values);

        if batch.len() >= Self::MIN_EAGER_ROWS {
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Get the checkpoint range from the batch
        let first_checkpoint = batch.first().unwrap().sequence_number;
        let last_checkpoint = batch.last().unwrap().sequence_number;
        let epoch = batch.first().unwrap().epoch;

        // Create a temporary Parquet file
        use crate::parquet::ParquetWriter;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let mut writer: ParquetWriter =
            ParquetWriter::new(temp_dir.path(), FileType::Checkpoint, first_checkpoint)?;

        // Collect into a vec to satisfy 'static lifetime requirement
        let rows: Vec<CheckpointEntry> = batch.to_vec();
        AnalyticsWriter::<CheckpointEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<CheckpointEntry>::flush(&mut writer, last_checkpoint + 1)?;

        // Build the object store path
        let file_path = FileType::Checkpoint.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        // Read the file and upload
        let local_file = temp_dir
            .path()
            .join(FileType::Checkpoint.dir_prefix().as_ref())
            .join(format!("epoch_{}", epoch))
            .join(format!(
                "{}_{}.parquet",
                first_checkpoint,
                last_checkpoint + 1
            ));

        let file_bytes = tokio::fs::read(&local_file).await?;

        conn.object_store()
            .put(&file_path, file_bytes.into())
            .await?;

        Ok(batch.len())
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
