// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{AnalyticsHandler, InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::FileType;

#[derive(Clone)]
pub struct TransactionObjectsHandler {}

impl TransactionObjectsHandler {
    pub fn new() -> Self {
        TransactionObjectsHandler {}
    }

    async fn process_transactions(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<TransactionObjectEntry>> {
        let txn_len = checkpoint_data.transactions.len();
        let mut entries = Vec::new();

        for idx in 0..txn_len {
            let transaction = &checkpoint_data.transactions[idx];
            let epoch = checkpoint_data.checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_data.checkpoint_summary.timestamp_ms;

            let transaction_digest = transaction.transaction.digest().base58_encode();
            let txn_data = transaction.transaction.transaction_data();
            let effects = &transaction.effects;

            let input_object_tracker = InputObjectTracker::new(txn_data);
            let object_status_tracker = ObjectStatusTracker::new(effects);

            // Process input objects
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
                entries.push(entry);
            }

            // Process output objects
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
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionObjectEntry> for TransactionObjectsHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<TransactionObjectEntry>> {
        self.process_transactions(checkpoint_data).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionObjects)
    }

    fn name(&self) -> &'static str {
        "transaction_objects"
    }
}
