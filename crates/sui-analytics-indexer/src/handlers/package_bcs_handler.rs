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

use crate::FileType;
use crate::tables::PackageBCSEntry;
use crate::writers::AnalyticsWriter;

pub struct PackageBCSHandler;

impl PackageBCSHandler {
    pub fn new() -> Self {
        Self
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
    type Batch = Vec<PackageBCSEntry>;

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
            ParquetWriter::new(temp_dir.path(), FileType::MovePackageBCS, first_checkpoint)?;

        // Collect into a vec to satisfy 'static lifetime requirement
        let rows: Vec<PackageBCSEntry> = batch.to_vec();
        AnalyticsWriter::<PackageBCSEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<PackageBCSEntry>::flush(&mut writer, last_checkpoint + 1)?;

        // Build the object store path
        let file_path = FileType::MovePackageBCS.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        // Read the file and upload
        let local_file = temp_dir
            .path()
            .join(FileType::MovePackageBCS.dir_prefix().as_ref())
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
