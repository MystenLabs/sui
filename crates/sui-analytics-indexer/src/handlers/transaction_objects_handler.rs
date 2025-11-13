// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::{AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata, FileType};

pub struct TransactionObjectsProcessor;

pub type TransactionObjectsHandler =
    AnalyticsHandler<TransactionObjectsProcessor, AnalyticsBatch<TransactionObjectEntry>>;

impl AnalyticsMetadata for TransactionObjectEntry {
    const FILE_TYPE: FileType = FileType::TransactionObjects;

    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint_sequence_number(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for TransactionObjectsProcessor {
    const NAME: &'static str = "transaction_objects";
    const FANOUT: usize = 10;
    type Value = TransactionObjectEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let mut entries = Vec::new();

        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        for transaction in &checkpoint.transactions {
            let effects = &transaction.effects;
            let transaction_digest_str = effects.transaction_digest().base58_encode();
            let txn_data = &transaction.transaction;

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
                    transaction_digest: transaction_digest_str.clone(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    input_kind: input_object_tracker.get_input_object_kind(&object_id),
                    object_status: object_status_tracker.get_object_status(&object_id),
                };
                entries.push(entry);
            }

            // Process output objects
            for object in transaction.output_objects(&checkpoint.object_set) {
                let object_id = object.id();
                let version = Some(object.version().value());
                let entry = TransactionObjectEntry {
                    object_id: object_id.to_string(),
                    version,
                    transaction_digest: transaction_digest_str.clone(),
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
