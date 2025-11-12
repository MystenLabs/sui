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
use crate::tables::MoveCallEntry;
use crate::writers::AnalyticsWriter;

pub struct MoveCallHandler;

impl MoveCallHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Processor for MoveCallHandler {
    const NAME: &'static str = "move_call";
    const FANOUT: usize = 10;
    type Value = MoveCallEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::new();

        for executed_tx in &checkpoint.transactions {
            let move_calls = executed_tx.transaction.move_calls();
            let transaction_digest = executed_tx.effects.transaction_digest().base58_encode();

            for (package, module, function) in move_calls.iter() {
                let entry = MoveCallEntry {
                    transaction_digest: transaction_digest.clone(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    package: package.to_string(),
                    module: module.to_string(),
                    function: function.to_string(),
                };
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

#[async_trait]
impl Handler for MoveCallHandler {
    type Store = ObjectStore;
    type Batch = Vec<MoveCallEntry>;

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
        let mut writer: ParquetWriter =
            ParquetWriter::new(temp_dir.path(), FileType::MoveCall, first_checkpoint)?;

        // Collect into a vec to satisfy 'static lifetime requirement
        let rows: Vec<MoveCallEntry> = batch.to_vec();
        AnalyticsWriter::<MoveCallEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<MoveCallEntry>::flush(&mut writer, last_checkpoint + 1)?;

        // Build the object store path
        let file_path = FileType::MoveCall.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        // Read the file and upload
        let local_file = temp_dir
            .path()
            .join(FileType::MoveCall.dir_prefix().as_ref())
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
