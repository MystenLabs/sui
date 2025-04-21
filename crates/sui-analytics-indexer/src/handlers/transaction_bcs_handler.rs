// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

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
        let mut state = self.state.lock().await;
        for checkpoint_transaction in checkpoint_transactions {
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
                &mut state,
            )?;
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
        let state = Mutex::new(State {
            transactions: vec![],
        });
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
