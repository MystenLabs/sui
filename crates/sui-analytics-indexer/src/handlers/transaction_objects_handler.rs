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

use crate::FileType;
use crate::handlers::{InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::writers::AnalyticsWriter;

pub struct TransactionObjectsHandler;

impl TransactionObjectsHandler {
    pub fn new() -> Self {
        Self
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
    type Batch = Vec<TransactionObjectEntry>;

    const MIN_EAGER_ROWS: usize = 100_000;
    const MAX_PENDING_ROWS: usize = 500_000;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        batch.extend(values);

        if batch.len() >= Self::MIN_EAGER_ROWS {
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Get the checkpoint range from the batch
        let first_checkpoint = batch.first().unwrap().checkpoint;
        let last_checkpoint = batch.last().unwrap().checkpoint;
        let epoch = batch.first().unwrap().epoch;

        // Create a temporary Parquet file
        use crate::parquet::ParquetWriter;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let mut writer: ParquetWriter = ParquetWriter::new(
            temp_dir.path(),
            FileType::TransactionObjects,
            first_checkpoint,
        )?;

        // Collect into a vec to satisfy 'static lifetime requirement
        let rows: Vec<TransactionObjectEntry> = batch.to_vec();
        AnalyticsWriter::<TransactionObjectEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<TransactionObjectEntry>::flush(&mut writer, last_checkpoint + 1)?;

        // Build the object store path
        let file_path = FileType::TransactionObjects.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        // Read the file and upload
        let local_file = temp_dir
            .path()
            .join(FileType::TransactionObjects.dir_prefix().as_ref())
            .join(format!("epoch_{}", epoch))
            .join(format!(
                "{}_{}.parquet",
                first_checkpoint,
                last_checkpoint + 1
            ));

        let file_bytes = tokio::fs::read(&local_file).await?;

        conn.object_store()
            .put(&file_path, file_bytes.into())
            .await?;

        Ok(batch.len())
    }
}
