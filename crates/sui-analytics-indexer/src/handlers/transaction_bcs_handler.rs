// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use futures::future::try_join_all;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};

use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};

use crate::handlers::AnalyticsHandler;
use crate::tables::TransactionBCSEntry;
use crate::FileType;

pub struct TransactionBCSHandler {
    pub(crate) state: Mutex<State>,
}

pub(crate) struct State {
    pub(crate) transactions: Vec<TransactionBCSEntry>,
}

#[async_trait::async_trait]
impl Worker for TransactionBCSHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;

        // Early-out if there’s nothing to do.
        if checkpoint_transactions.is_empty() {
            return Ok(());
        }

        // ──────────────────────────────────────────────────────────────────────────
        // Concurrency infrastructure – one semaphore per transaction
        // ──────────────────────────────────────────────────────────────────────────
        let n = checkpoint_transactions.len();
        let semaphores: Vec<Arc<Semaphore>> = (0..n).map(|_| Arc::new(Semaphore::new(0))).collect();
        semaphores[0].add_permits(1); // kick-start the chain

        let epoch = checkpoint_summary.epoch;
        let checkpoint = checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint_summary.timestamp_ms;

        // Own the transactions so they can move into async blocks cleanly.
        let transactions: Vec<CheckpointTransaction> = checkpoint_transactions.clone();

        let mut futs = Vec::with_capacity(n);
        for (idx, tx) in transactions.into_iter().enumerate() {
            let sem_curr = semaphores[idx].clone();
            let sem_next = semaphores.get(idx + 1).cloned();
            let handler_ref = self; // capture &self by value into the task

            futs.push(async move {
                // Build the entry completely off the shared state.
                let transaction = &tx.transaction;
                let txn_data = transaction.transaction_data();
                let transaction_digest = transaction.digest().base58_encode();

                let entry = TransactionBCSEntry {
                    transaction_digest,
                    checkpoint,
                    epoch,
                    timestamp_ms,
                    bcs: Base64::encode(bcs::to_bytes(&txn_data).unwrap()),
                };

                // Wait for our turn to push into the shared vector.
                sem_curr.acquire().await.unwrap().forget();

                {
                    let mut state = handler_ref.state.lock().await;
                    state.transactions.push(entry);
                }

                // Signal the next transaction (if any).
                if let Some(next) = sem_next {
                    next.add_permits(1);
                }

                Ok::<(), anyhow::Error>(())
            });
        }

        // Drive all tasks concurrently; bubble up the first error if any.
        try_join_all(futs).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionBCSEntry> for TransactionBCSHandler {
    async fn read(&self) -> Result<Vec<TransactionBCSEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.transactions))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionBCS)
    }

    fn name(&self) -> &str {
        "transaction_bcs"
    }
}

impl TransactionBCSHandler {
    pub fn new() -> Self {
        let state = Mutex::new(State {
            transactions: Vec::new(),
        });
        TransactionBCSHandler { state }
    }
}

#[cfg(test)]
mod tests {
    use crate::handlers::transaction_bcs_handler::TransactionBCSHandler;
    use fastcrypto::encoding::{Base64, Encoding};
    use simulacrum::Simulacrum;
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
        txn_handler.process_checkpoint(&checkpoint_data).await?;
        let transaction_entries = txn_handler.state.lock().await.transactions.clone();
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
