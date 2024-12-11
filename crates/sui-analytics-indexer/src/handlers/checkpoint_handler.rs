// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::traits::EncodeDecodeBase64;
use tokio::sync::Mutex;

use sui_data_ingestion_core::Worker;
use sui_rpc_api::{CheckpointData, CheckpointTransaction};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSummary};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::CheckpointEntry;
use crate::FileType;

pub struct CheckpointHandler {
    state: Mutex<State>,
}

struct State {
    checkpoints: Vec<CheckpointEntry>,
}

#[async_trait::async_trait]
impl Worker for CheckpointHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        self.process_checkpoint_transactions(checkpoint_summary, checkpoint_transactions)
            .await;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<CheckpointEntry> for CheckpointHandler {
    async fn read(&self) -> Result<Vec<CheckpointEntry>> {
        let mut state = self.state.lock().await;
        let cloned = state.checkpoints.clone();
        state.checkpoints.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Checkpoint)
    }

    fn name(&self) -> &str {
        "checkpoint"
    }
}

impl CheckpointHandler {
    pub fn new() -> Self {
        CheckpointHandler {
            state: Mutex::new(State {
                checkpoints: vec![],
            }),
        }
    }
    async fn process_checkpoint_transactions(
        &self,
        summary: &CertifiedCheckpointSummary,
        checkpoint_transactions: &[CheckpointTransaction],
    ) {
        let CheckpointSummary {
            epoch,
            sequence_number,
            network_total_transactions,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            end_of_epoch_data,
            ..
        } = summary.data();

        let total_gas_cost = epoch_rolling_gas_cost_summary.computation_cost as i64
            + epoch_rolling_gas_cost_summary.storage_cost as i64
            - epoch_rolling_gas_cost_summary.storage_rebate as i64;
        let total_transaction_blocks = checkpoint_transactions.len() as u64;
        let mut total_transactions: u64 = 0;
        let mut total_successful_transaction_blocks: u64 = 0;
        let mut total_successful_transactions: u64 = 0;
        for checkpoint_transaction in checkpoint_transactions {
            let txn_data = checkpoint_transaction.transaction.transaction_data();
            let cmds = txn_data.kind().num_commands() as u64;
            total_transactions += cmds;
            if checkpoint_transaction.effects.status().is_ok() {
                total_successful_transaction_blocks += 1;
                total_successful_transactions += cmds;
            }
        }

        let checkpoint_entry = CheckpointEntry {
            sequence_number: *sequence_number,
            checkpoint_digest: summary.digest().base58_encode(),
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
            validator_signature: summary.auth_sig().signature.encode_base64(),
        };
        let mut state = self.state.lock().await;
        state.checkpoints.push(checkpoint_entry);
    }
}
