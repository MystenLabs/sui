// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::MoveCallEntry;
use crate::FileType;

const NAME: &str = "move_call";

#[derive(Clone)]
pub struct MoveCallHandler {}

impl MoveCallHandler {
    pub fn new() -> Self {
        Self {}
    }

    async fn process_transactions(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<MoveCallEntry>> {
        let txn_len = checkpoint_data.transactions.len();
        let mut entries = Vec::new();

        for idx in 0..txn_len {
            let transaction = &checkpoint_data.transactions[idx];
            let move_calls = transaction.transaction.transaction_data().move_calls();
            let epoch = checkpoint_data.checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_data.checkpoint_summary.timestamp_ms;
            let transaction_digest = transaction.transaction.digest().base58_encode();

            for (package, module, function) in move_calls.iter() {
                let entry = MoveCallEntry {
                    transaction_digest: transaction_digest.clone(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    package: package.to_string(),
                    module: module.to_string(),
                    function: function.to_string(),
                };
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MoveCallEntry> for MoveCallHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<MoveCallEntry>> {
        self.process_transactions(checkpoint_data).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MoveCall)
    }

    fn name(&self) -> &'static str {
        NAME
    }
}
