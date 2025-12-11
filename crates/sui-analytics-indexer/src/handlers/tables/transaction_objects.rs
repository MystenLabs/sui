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

use super::{InputObjectTracker, ObjectStatusTracker};
use crate::Row;
use crate::tables::TransactionObjectRow;

pub struct TransactionObjectsProcessor;

impl Row for TransactionObjectRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for TransactionObjectsProcessor {
    const NAME: &'static str = "transaction_objects";
    const FANOUT: usize = 10;
    type Value = TransactionObjectRow;

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
                let row = TransactionObjectRow {
                    object_id: object_id.to_string(),
                    version,
                    transaction_digest: transaction_digest_str.clone(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    input_kind: input_object_tracker.get_input_object_kind(&object_id),
                    object_status: object_status_tracker.get_object_status(&object_id),
                };
                entries.push(row);
            }

            // Process output objects
            for object in transaction.output_objects(&checkpoint.object_set) {
                let object_id = object.id();
                let version = Some(object.version().value());
                let row = TransactionObjectRow {
                    object_id: object_id.to_string(),
                    version,
                    transaction_digest: transaction_digest_str.clone(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    input_kind: input_object_tracker.get_input_object_kind(&object_id),
                    object_status: object_status_tracker.get_object_status(&object_id),
                };
                entries.push(row);
            }
        }

        Ok(entries)
    }
}
