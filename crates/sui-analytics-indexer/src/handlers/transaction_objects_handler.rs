// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use sui_indexer::framework::Handler;
use sui_rest_api::{CheckpointData, CheckpointTransaction};
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{AnalyticsHandler, InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::FileType;

pub struct TransactionObjectsHandler {
    transaction_objects: Vec<TransactionObjectEntry>,
}

#[async_trait::async_trait]
impl Handler for TransactionObjectsHandler {
    fn name(&self) -> &str {
        "transaction_objects"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        for checkpoint_transaction in checkpoint_transactions {
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
                &checkpoint_transaction.effects,
            );
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionObjectEntry> for TransactionObjectsHandler {
    fn read(&mut self) -> Result<Vec<TransactionObjectEntry>> {
        let cloned = self.transaction_objects.clone();
        self.transaction_objects.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionObjects)
    }
}

impl TransactionObjectsHandler {
    pub fn new() -> Self {
        TransactionObjectsHandler {
            transaction_objects: vec![],
        }
    }
    fn process_transaction(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
    ) {
        let transaction = &checkpoint_transaction.transaction;
        let transaction_digest = transaction.digest().base58_encode();
        let txn_data = transaction.transaction_data();
        let input_object_tracker = InputObjectTracker::new(txn_data);
        let object_status_tracker = ObjectStatusTracker::new(effects);
        // input
        txn_data
            .input_objects()
            .expect("Input objects must be valid")
            .iter()
            .map(|object| (object.object_id(), object.version().map(|v| v.value())))
            .for_each(|(object_id, version)| {
                self.process_transaction_object(
                    epoch,
                    checkpoint,
                    timestamp_ms,
                    transaction_digest.clone(),
                    &object_id,
                    version,
                    &input_object_tracker,
                    &object_status_tracker,
                )
            });
        // output
        checkpoint_transaction
            .output_objects
            .iter()
            .map(|object| (object.id(), Some(object.version().value())))
            .for_each(|(object_id, version)| {
                self.process_transaction_object(
                    epoch,
                    checkpoint,
                    timestamp_ms,
                    transaction_digest.clone(),
                    &object_id,
                    version,
                    &input_object_tracker,
                    &object_status_tracker,
                )
            });
    }
    // Transaction object data.
    // Builds a view of the object in input and output of a transaction.
    fn process_transaction_object(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        transaction_digest: String,
        object_id: &ObjectID,
        version: Option<u64>,
        input_object_tracker: &InputObjectTracker,
        object_status_tracker: &ObjectStatusTracker,
    ) {
        let entry = TransactionObjectEntry {
            object_id: object_id.to_string(),
            version,
            transaction_digest,
            checkpoint,
            epoch,
            timestamp_ms,
            input_kind: input_object_tracker.get_input_object_kind(object_id),
            object_status: object_status_tracker.get_object_status(object_id),
        };
        self.transaction_objects.push(entry);
    }
}
