// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};

use sui_types::full_checkpoint_content::CheckpointData;

use crate::FileType;
use crate::handlers::{AnalyticsHandler, TransactionProcessor, process_transactions};
use crate::tables::TransactionBCSEntry;

#[derive(Clone)]
pub struct TransactionBCSHandler {}

impl TransactionBCSHandler {
    pub fn new() -> Self {
        TransactionBCSHandler {}
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionBCSEntry> for TransactionBCSHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &Arc<CheckpointData>,
    ) -> Result<Box<dyn Iterator<Item = TransactionBCSEntry> + Send + Sync>> {
        process_transactions(checkpoint_data.clone(), Arc::new(self.clone())).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionBCS)
    }

    fn name(&self) -> &'static str {
        "transaction_bcs"
    }
}

#[async_trait::async_trait]
impl TransactionProcessor<TransactionBCSEntry> for TransactionBCSHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Box<dyn Iterator<Item = TransactionBCSEntry> + Send + Sync>> {
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

        Ok(Box::new(std::iter::once(entry)))
    }
}

#[cfg(test)]
mod tests {
    use crate::handlers::AnalyticsHandler;
    use crate::handlers::transaction_bcs_handler::TransactionBCSHandler;
    use fastcrypto::encoding::{Base64, Encoding};
    use simulacrum::Simulacrum;
    use std::sync::Arc;
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
        let checkpoint_data: sui_types::full_checkpoint_content::CheckpointData = sim
            .get_checkpoint_data(
                checkpoint.clone(),
                sim.get_checkpoint_contents_by_digest(&checkpoint.content_digest)
                    .unwrap(),
            )?
            .into();
        let txn_handler = TransactionBCSHandler::new();
        let transaction_entries: Vec<_> = txn_handler
            .process_checkpoint(&Arc::new(checkpoint_data))
            .await?
            .collect();
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
