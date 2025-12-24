// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::FileType;
use crate::handlers::{AnalyticsHandler, TransactionProcessor, process_transactions};
use crate::tables::MoveCallEntry;

const NAME: &str = "move_call";

#[derive(Clone)]
pub struct MoveCallHandler {}

impl MoveCallHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MoveCallEntry> for MoveCallHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &Arc<CheckpointData>,
    ) -> Result<Box<dyn Iterator<Item = MoveCallEntry> + Send + Sync>> {
        process_transactions(checkpoint_data.clone(), Arc::new(self.clone())).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MoveCall)
    }

    fn name(&self) -> &'static str {
        NAME
    }
}

#[async_trait::async_trait]
impl TransactionProcessor<MoveCallEntry> for MoveCallHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Box<dyn Iterator<Item = MoveCallEntry> + Send + Sync>> {
        let transaction = &checkpoint.transactions[tx_idx];
        let move_calls = transaction.transaction.transaction_data().move_calls();
        let epoch = checkpoint.checkpoint_summary.epoch;
        let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;
        let transaction_digest = transaction.transaction.digest().base58_encode();

        let mut entries = Vec::new();
        for (cmd_idx, package, module, function) in move_calls.iter() {
            let entry = MoveCallEntry {
                transaction_digest: transaction_digest.clone(),
                cmd_idx: *cmd_idx as u64,
                checkpoint: checkpoint_seq,
                epoch,
                timestamp_ms,
                package: package.to_string(),
                module: module.to_string(),
                function: function.to_string(),
            };
            entries.push(entry);
        }

        Ok(Box::new(entries.into_iter()))
    }
}
