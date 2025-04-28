// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use futures::{stream, StreamExt};
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use sui_types::full_checkpoint_content::CheckpointData;

use crate::handlers::AnalyticsHandler;
use crate::tables::TransactionBCSEntry;
use crate::FileType;

#[derive(Clone)]
pub struct TransactionBCSHandler {
    pub(crate) state: Arc<Mutex<Vec<TransactionBCSEntry>>>,
}

#[async_trait::async_trait]
impl Worker for TransactionBCSHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        // Process transactions in parallel using buffered stream for ordered execution
        let txn_len = checkpoint_data.transactions.len();
        let mut entries = Vec::new();
        
        let mut stream = stream::iter(0..txn_len)
            .map(|idx| {
                let cp = checkpoint_data.clone();
                tokio::spawn(async move { 
                    handle_tx(idx, &cp).await
                })
            })
            .buffered(num_cpus::get() * 4);

        while let Some(join_res) = stream.next().await {
            match join_res {
                Ok(Ok(tx_entries)) => {
                    entries.extend(tx_entries);
                }
                Ok(Err(e)) => {
                    // Task executed but application logic returned an error
                    return Err(e);
                }
                Err(e) => {
                    // Task panicked or was cancelled
                    return Err(anyhow::anyhow!("Task join error: {}", e));
                }
            }
        }

        // Store results
        *self.state.lock().await = entries;
        Ok(())
    }
}

/// Private per-tx helper for processing individual transactions
async fn handle_tx(
    tx_idx: usize, 
    checkpoint: &CheckpointData,
) -> Result<Vec<TransactionBCSEntry>> {
    let transaction = &checkpoint.transactions[tx_idx];
    let epoch = checkpoint.checkpoint_summary.epoch;
    let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
    let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

    let txn = &transaction.transaction;
    let txn_data = txn.transaction_data();
    let transaction_digest = txn.digest().base58_encode();

    let entry = TransactionBCSEntry {
        transaction_digest,
        checkpoint: checkpoint_seq,
        epoch,
        timestamp_ms,
        bcs: Base64::encode(bcs::to_bytes(&txn_data).unwrap()),
    };

    Ok(vec![entry])
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionBCSEntry> for TransactionBCSHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = TransactionBCSEntry>>> {
        let mut state = self.state.lock().await;
        let entries = std::mem::take(&mut *state);

        // Return all entries
        Ok(Box::new(entries.into_iter()))
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
            state: Arc::new(Mutex::new(Vec::new())),
        }
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
        let transactions = txn_handler.state.lock().await;
        let transaction_entries: Vec<_> = transactions.clone();
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
