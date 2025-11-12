// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::PipelineConfig;
use crate::handlers::{InputObjectTracker, ObjectStatusTracker};
use crate::parquet::ParquetBatch;
use crate::tables::TransactionObjectEntry;

pub struct TransactionObjectsHandler {
    config: PipelineConfig,
}

impl TransactionObjectsHandler {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Processor for TransactionObjectsHandler {
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

#[async_trait]
impl Handler for TransactionObjectsHandler {
    type Store = ObjectStore;
    type Batch = ParquetBatch<TransactionObjectEntry>;

    const MIN_EAGER_ROWS: usize = usize::MAX;
    const MAX_PENDING_ROWS: usize = usize::MAX;

    fn min_eager_rows(&self) -> usize {
        self.config.max_row_count
    }

    fn max_pending_rows(&self) -> usize {
        self.config.max_row_count * 5
    }

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        // Get first value to extract epoch and checkpoint
        let Some(first) = values.next() else {
            return BatchStatus::Pending;
        };

        batch.set_epoch(first.epoch);
        batch.update_last_checkpoint(first.checkpoint);

        // Write first value and remaining values
        if let Err(e) = batch.write_rows(std::iter::once(first).chain(values.by_ref()), crate::FileType::TransactionObjects) {
            tracing::error!("Failed to write rows to ParquetBatch: {}", e);
            return BatchStatus::Pending;
        }

        // Let framework decide when to flush based on min_eager_rows()
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let Some(file_path) = batch.current_file_path() else {
            return Ok(0);
        };

        let row_count = batch.row_count()?;
        let file_bytes = tokio::fs::read(file_path).await?;
        let object_path = batch.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}
