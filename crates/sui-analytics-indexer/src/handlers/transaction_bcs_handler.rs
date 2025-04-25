// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};

use crate::handlers::AnalyticsHandler;
use crate::tables::TransactionBCSEntry;
use crate::FileType;

pub struct TransactionBCSHandler {
    pub(crate) state: Mutex<BTreeMap<usize, Vec<TransactionBCSEntry>>>,
}

#[async_trait::async_trait]
impl Worker for TransactionBCSHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let checkpoint_summary = &checkpoint_data.checkpoint_summary;
        let checkpoint_transactions = &checkpoint_data.transactions;

        // Create a channel to collect results
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, Vec<TransactionBCSEntry>)>(
            checkpoint_transactions.len(),
        );

        // Process transactions in parallel
        let mut futures = Vec::new();

        for (idx, _checkpoint_transaction) in checkpoint_transactions.iter().enumerate() {
            let tx = tx.clone();
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;
            let checkpoint_data_clone = checkpoint_data.clone();

            // Spawn a task for each transaction
            let handle = tokio::spawn(async move {
                let transaction = &checkpoint_data_clone.transactions[idx];
                match Self::process_transaction(epoch, checkpoint_seq, timestamp_ms, transaction) {
                    Ok(entries) => {
                        if !entries.is_empty() {
                            let _ = tx.send((idx, entries)).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error processing transaction at index {}: {}", idx, e);
                    }
                }
            });

            futures.push(handle);
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
        while let Some((idx, transactions)) = rx.recv().await {
            state.insert(idx, transactions);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionBCSEntry> for TransactionBCSHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = TransactionBCSEntry>>> {
        let mut state = self.state.lock().await;
        let transactions_map = std::mem::take(&mut *state);

        // Flatten the map into a single iterator in order by transaction index
        Ok(Box::new(transactions_map.into_values().flatten()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionBCS)
    }

    fn name(&self) -> &'static str {
        "transaction_bcs"
    }
}

impl TransactionBCSHandler {
    pub fn new() -> Self {
        TransactionBCSHandler {
            state: Mutex::new(BTreeMap::new()),
        }
    }

    fn process_transaction(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
    ) -> Result<Vec<TransactionBCSEntry>> {
        let transaction = &checkpoint_transaction.transaction;
        let txn_data = transaction.transaction_data();
        let transaction_digest = transaction.digest().base58_encode();

        let entry = TransactionBCSEntry {
            transaction_digest,
            checkpoint,
            epoch,
            timestamp_ms,
            bcs: Base64::encode(bcs::to_bytes(&txn_data).unwrap()),
        };

        Ok(vec![entry])
    }
}

#[cfg(test)]
mod tests {
    use crate::handlers::transaction_bcs_handler::TransactionBCSHandler;
    use fastcrypto::encoding::{Base64, Encoding};
    use simulacrum::Simulacrum;
    use std::sync::Arc;
    use sui_data_ingestion_core::Worker;
    use sui_types::base_types::SuiAddress;
    use sui_types::storage::ReadStore;

    #[tokio::test]
    pub async fn test_transaction_bcs_handler() -> anyhow::Result<()> {
        let mut sim = Simulacrum::new();

        // Execute a simple transaction.
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (_effects, err) = sim.execute_transaction(transaction.clone()).unwrap();
        assert!(err.is_none());

        // Create a checkpoint which should include the transaction we executed.
        let checkpoint = sim.create_checkpoint();
        let checkpoint_data = sim.get_checkpoint_data(
            checkpoint.clone(),
            sim.get_checkpoint_contents_by_digest(&checkpoint.content_digest)
                .unwrap(),
        )?;
        let txn_handler = TransactionBCSHandler::new();
        txn_handler
            .process_checkpoint(Arc::new(checkpoint_data))
            .await?;

        // Extract entries from state
        let transaction_map = txn_handler.state.lock().await;
        let transaction_entries: Vec<_> = transaction_map.values().flatten().cloned().collect();
        assert_eq!(transaction_entries.len(), 1);
        let db_txn = transaction_entries.first().unwrap();

        // Check that the transaction was stored correctly.
        assert_eq!(db_txn.transaction_digest, transaction.digest().to_string());
        assert_eq!(
            db_txn.bcs,
            Base64::encode(bcs::to_bytes(&transaction.transaction_data()).unwrap())
        );
        assert_eq!(db_txn.epoch, checkpoint.epoch);
        assert_eq!(db_txn.timestamp_ms, checkpoint.timestamp_ms);
        assert_eq!(db_txn.checkpoint, checkpoint.sequence_number);
        Ok(())
    }
}
