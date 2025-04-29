// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{
    process_transactions, AnalyticsHandler, InputObjectTracker, ObjectStatusTracker, TxProcessor,
};
use crate::tables::TransactionObjectEntry;
use crate::FileType;

#[derive(Clone)]
pub struct TransactionObjectsHandler {
    state: Arc<Mutex<Vec<TransactionObjectEntry>>>,
}

#[async_trait::async_trait]
impl Worker for TransactionObjectsHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let results = process_transactions(checkpoint_data, Arc::new(self.clone())).await?;
        *self.state.lock().await = results;
        Ok(())
    }
}

#[async_trait::async_trait]
impl TxProcessor<TransactionObjectEntry> for TransactionObjectsHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Vec<TransactionObjectEntry>> {
        let transaction = &checkpoint.transactions[tx_idx];
        let epoch = checkpoint.checkpoint_summary.epoch;
        let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

        let transaction_digest = transaction.transaction.digest().base58_encode();
        let txn_data = transaction.transaction.transaction_data();
        let effects = &transaction.effects;

        let input_object_tracker = InputObjectTracker::new(txn_data);
        let object_status_tracker = ObjectStatusTracker::new(effects);
        let mut transaction_objects = Vec::new();

        // input
        for object in txn_data
            .input_objects()
            .expect("Input objects must be valid")
            .iter()
        {
            let object_id = object.object_id();
            let version = object.version().map(|v| v.value());
            let entry = TransactionObjectEntry {
                object_id: object_id.to_string(),
                version,
                transaction_digest: transaction_digest.clone(),
                checkpoint: checkpoint_seq,
                epoch,
                timestamp_ms,
                input_kind: input_object_tracker.get_input_object_kind(&object_id),
                object_status: object_status_tracker.get_object_status(&object_id),
            };
            transaction_objects.push(entry);
        }

        // output
        for object in transaction.output_objects.iter() {
            let object_id = object.id();
            let version = Some(object.version().value());
            let entry = TransactionObjectEntry {
                object_id: object_id.to_string(),
                version,
                transaction_digest: transaction_digest.clone(),
                checkpoint: checkpoint_seq,
                epoch,
                timestamp_ms,
                input_kind: input_object_tracker.get_input_object_kind(&object_id),
                object_status: object_status_tracker.get_object_status(&object_id),
            };
            transaction_objects.push(entry);
        }

        Ok(transaction_objects)
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionObjectEntry> for TransactionObjectsHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = TransactionObjectEntry>>> {
        let mut state = self.state.lock().await;
        Ok(Box::new(std::mem::take(&mut *state).into_iter()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionObjects)
    }

    fn name(&self) -> &'static str {
        "transaction_objects"
    }
}

impl TransactionObjectsHandler {
    pub fn new() -> Self {
        TransactionObjectsHandler {
            state: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
