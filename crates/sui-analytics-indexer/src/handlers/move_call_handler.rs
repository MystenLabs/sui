// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{process_transactions, AnalyticsHandler, TxProcessor};
use crate::tables::MoveCallEntry;
use crate::FileType;

#[derive(Clone)]
pub struct MoveCallHandler {
    state: Arc<Mutex<Vec<MoveCallEntry>>>,
}

const NAME: &str = "move_call";

#[async_trait::async_trait]
impl Worker for MoveCallHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let results = process_transactions(checkpoint_data, Arc::new(self.clone())).await?;
        *self.state.lock().await = results;
        Ok(())
    }
}

#[async_trait::async_trait]
impl TxProcessor<MoveCallEntry> for MoveCallHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Vec<MoveCallEntry>> {
        let transaction = &checkpoint.transactions[tx_idx];
        let move_calls = transaction.transaction.transaction_data().move_calls();

        // Skip if no move calls
        if move_calls.is_empty() {
            return Ok(Vec::new());
        }

        let epoch = checkpoint.checkpoint_summary.epoch;
        let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;
        let transaction_digest = transaction.transaction.digest().base58_encode();

        let mut entries = Vec::new();
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

        Ok(entries)
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MoveCallEntry> for MoveCallHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = MoveCallEntry>>> {
        let mut state = self.state.lock().await;
        Ok(Box::new(std::mem::take(&mut *state).into_iter()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MoveCall)
    }

    fn name(&self) -> &'static str {
        NAME
    }
}

impl MoveCallHandler {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
