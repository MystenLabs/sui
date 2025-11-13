// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::traits::EncodeDecodeBase64;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::parquet::ParquetBatch;
use crate::tables::CheckpointEntry;
use crate::{AnalyticsBatch, AnalyticsHandler, CheckpointMetadata, FileType};

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

// Implement traits for composition pattern
impl CheckpointMetadata for CheckpointEntry {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint_sequence_number(&self) -> u64 {
        self.sequence_number
    }
}

impl AnalyticsBatch for CheckpointBatch {
    type Entry = CheckpointEntry;

    fn inner_mut(&mut self) -> &mut ParquetBatch<Self::Entry> {
        &mut self.inner
    }

    fn inner(&self) -> &ParquetBatch<Self::Entry> {
        &self.inner
    }
}

// The processor contains only processing logic, no config
pub struct CheckpointProcessor;

#[async_trait]
impl Processor for CheckpointProcessor {
    const NAME: &'static str = "checkpoint";
    const FANOUT: usize = 10;
    type Value = CheckpointEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let checkpoint_entry = process_checkpoint(checkpoint);
        Ok(vec![checkpoint_entry])
    }
}

// Type alias for backward compatibility
pub type CheckpointHandler = AnalyticsHandler<CheckpointProcessor, CheckpointBatch>;

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
