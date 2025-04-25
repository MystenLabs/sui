// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::MoveCallEntry;
use crate::FileType;

pub struct MoveCallHandler {
    state: Mutex<BTreeMap<usize, Vec<MoveCallEntry>>>,
}

const NAME: &str = "move_call";

#[async_trait::async_trait]
impl Worker for MoveCallHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let checkpoint_summary = &checkpoint_data.checkpoint_summary;
        let checkpoint_transactions = &checkpoint_data.transactions;

        // Create a channel to collect results
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, Vec<MoveCallEntry>)>(
            checkpoint_transactions.len(),
        );

        // Process transactions in parallel
        let mut futures = Vec::new();

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().enumerate() {
            // Clone the checkpoint transaction
            let transaction = checkpoint_transaction.clone();

            if !transaction
                .transaction
                .transaction_data()
                .move_calls()
                .is_empty()
            {
                let tx = tx.clone();
                let epoch = checkpoint_summary.epoch;
                let checkpoint_seq = checkpoint_summary.sequence_number;
                let timestamp_ms = checkpoint_summary.timestamp_ms;
                let transaction_digest = transaction.transaction.digest().base58_encode();

                // Spawn a task for each transaction
                let handle = tokio::spawn(async move {
                    let entries = Self::process_move_calls(
                        epoch,
                        checkpoint_seq,
                        timestamp_ms,
                        transaction_digest,
                        &transaction.transaction.transaction_data().move_calls(),
                    );

                    if !entries.is_empty() {
                        let _ = tx.send((idx, entries)).await;
                    }
                });

                futures.push(handle);
            }
        }

        // Drop the original sender so the channel can close when all tasks are done
        drop(tx);

        // Wait for all tasks to complete
        for handle in futures {
            if let Err(e) = handle.await {
                tracing::error!("Task panicked: {}", e);
            }
        }

        // Collect results into the state in order by transaction index
        let mut state = self.state.lock().await;
        while let Some((idx, entries)) = rx.recv().await {
            state.insert(idx, entries);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MoveCallEntry> for MoveCallHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = MoveCallEntry>>> {
        let mut state = self.state.lock().await;
        let move_calls_map = std::mem::take(&mut *state);

        // Flatten the map into a single iterator in order by transaction index
        Ok(Box::new(move_calls_map.into_values().flatten()))
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
            state: Mutex::new(BTreeMap::new()),
        }
    }

    fn process_move_calls(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        transaction_digest: String,
        move_calls: &[(&ObjectID, &str, &str)],
    ) -> Vec<MoveCallEntry> {
        let mut entries = Vec::new();
        for (package, module, function) in move_calls.iter() {
            let entry = MoveCallEntry {
                transaction_digest: transaction_digest.clone(),
                checkpoint,
                epoch,
                timestamp_ms,
                package: package.to_string(),
                module: module.to_string(),
                function: function.to_string(),
            };
            entries.push(entry);
        }
        entries
    }
}
