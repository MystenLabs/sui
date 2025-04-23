// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};

use crate::handlers::AnalyticsHandler;
use crate::tables::TransactionBCSEntry;
use crate::FileType;

#[derive(Clone)]
pub struct TransactionBCSHandler {
    pub(crate) state: Arc<Mutex<State>>,
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

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.transactions` in the
        // same order as `checkpoint_transactions`, while allowing *everything*
        // else to run in parallel.
        // ──────────────────────────────────────────────────────────────────────────
        let txn_count = checkpoint_transactions.len();
        let semaphores: Vec<_> = (0..txn_count)
            .map(|i| {
                if i == 0 {
                    Arc::new(Semaphore::new(1)) // first txn proceeds immediately
                } else {
                    Arc::new(Semaphore::new(0))
                }
            })
            .collect();

        let mut handles: Vec<JoinHandle<Result<()>>> = Vec::with_capacity(txn_count);

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().cloned().enumerate() {
            let handler = self.clone();
            let start_sem = semaphores[idx].clone();
            let next_sem = semaphores.get(idx + 1).cloned();

            // Snapshot any data we need from the summary (Copy types, cheap).
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;

            let handle = tokio::spawn(async move {
                // ───── 1. Heavy work off‑mutex ───────────────────────────────────
                let mut local_state = State {
                    transactions: Vec::new(),
                };

                handler.process_transaction(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    &checkpoint_transaction,
                    &mut local_state,
                )?;

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _permit = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state
                        .transactions
                        .extend(local_state.transactions.into_iter());
                }

                // Signal the next task.
                if let Some(next) = next_sem {
                    next.add_permits(1);
                }

                Ok(())
            });

            handles.push(handle);
        }

        // Propagate any error.
        for h in handles {
            h.await??;
        }

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
        let state = Arc::new(Mutex::new(State {
            transactions: vec![],
        }));
        TransactionBCSHandler { state }
    }
    fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        state: &mut State,
    ) -> Result<()> {
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
        state.transactions.push(entry);
        Ok(())
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
