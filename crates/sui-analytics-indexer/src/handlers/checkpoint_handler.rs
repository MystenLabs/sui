// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::traits::EncodeDecodeBase64;
use std::sync::Arc;

use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::CheckpointEntry;
use crate::FileType;

pub struct CheckpointHandler {}

impl CheckpointHandler {
    pub fn new() -> Self {
        CheckpointHandler {}
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<CheckpointEntry> for CheckpointHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &Arc<CheckpointData>,
    ) -> Result<Vec<CheckpointEntry>> {
        let checkpoint_entry = process_checkpoint_data(checkpoint_data);
        Ok(vec![checkpoint_entry])
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Checkpoint)
    }

    fn name(&self) -> &'static str {
        "checkpoint"
    }
}

fn process_checkpoint_data(checkpoint_data: &CheckpointData) -> CheckpointEntry {
    let CheckpointSummary {
        epoch,
        sequence_number,
        network_total_transactions,
        previous_digest,
        epoch_rolling_gas_cost_summary,
        timestamp_ms,
        end_of_epoch_data,
        ..
    } = checkpoint_data.checkpoint_summary.data();

    let total_gas_cost = epoch_rolling_gas_cost_summary.computation_cost as i64
        + epoch_rolling_gas_cost_summary.storage_cost as i64
        - epoch_rolling_gas_cost_summary.storage_rebate as i64;
    let total_transaction_blocks = checkpoint_data.transactions.len() as u64;
    let mut total_transactions: u64 = 0;
    let mut total_successful_transaction_blocks: u64 = 0;
    let mut total_successful_transactions: u64 = 0;
    for checkpoint_transaction in &checkpoint_data.transactions {
        let txn_data = checkpoint_transaction.transaction.transaction_data();
        let cmds = txn_data.kind().num_commands() as u64;
        total_transactions += cmds;
        if checkpoint_transaction.effects.status().is_ok() {
            total_successful_transaction_blocks += 1;
            total_successful_transactions += cmds;
        }
    }

    CheckpointEntry {
        sequence_number: *sequence_number,
        checkpoint_digest: checkpoint_data.checkpoint_summary.digest().base58_encode(),
        previous_checkpoint_digest: previous_digest.map(|d| d.base58_encode()),
        epoch: *epoch,
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
        network_total_transaction: *network_total_transactions,
        timestamp_ms: *timestamp_ms,
        validator_signature: checkpoint_data
            .checkpoint_summary
            .auth_sig()
            .signature
            .encode_base64(),
    }
}
