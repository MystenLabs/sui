// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::TaskConfig;
use crate::parquet::ParquetBatch;
use crate::tables::PackageBCSEntry;

pub struct PackageBCSHandler {
    config: TaskConfig,
}

impl PackageBCSHandler {
    pub fn new(config: TaskConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Processor for PackageBCSHandler {
    const NAME: &'static str = "move_package_bcs";
    const FANOUT: usize = 10;
    type Value = PackageBCSEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut packages = Vec::new();
        for transaction in &checkpoint.transactions {
            for object in transaction.output_objects(&checkpoint.object_set) {
                if let sui_types::object::Data::Package(_p) = &object.data {
                    let package_id = object.id();
                    let entry = PackageBCSEntry {
                        package_id: package_id.to_string(),
                        checkpoint: checkpoint_seq,
                        epoch,
                        timestamp_ms,
                        bcs: Base64::encode(bcs::to_bytes(object).unwrap()),
                    };
                    packages.push(entry);
                }
            }
        }

        Ok(packages)
    }
}

#[async_trait]
impl Handler for PackageBCSHandler {
    type Store = ObjectStore;
    type Batch = ParquetBatch<PackageBCSEntry>;

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
        if let Err(e) = batch.write_rows(std::iter::once(first).chain(values.by_ref())) {
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
