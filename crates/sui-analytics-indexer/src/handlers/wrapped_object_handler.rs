// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::FileType;
use crate::handlers::{get_move_struct, parse_struct};
use crate::package_store::PackageCache;
use crate::tables::WrappedObjectEntry;
use crate::writers::AnalyticsWriter;

pub struct WrappedObjectHandler {
    package_cache: Arc<PackageCache>,
}

impl WrappedObjectHandler {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }
}

#[async_trait]
impl Processor for WrappedObjectHandler {
    const NAME: &'static str = "wrapped_object";
    const FANOUT: usize = 10;
    type Value = WrappedObjectEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut wrapped_objects = Vec::new();

        for transaction in &checkpoint.transactions {
            for object in transaction.output_objects(&checkpoint.object_set) {
                let move_struct = if let Some((tag, contents)) = object
                    .struct_tag()
                    .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
                {
                    match get_move_struct(
                        &tag,
                        contents,
                        &self.package_cache.resolver_for_epoch(epoch),
                    )
                    .await
                    {
                        Ok(move_struct) => Some(move_struct),
                        Err(err)
                            if err
                                .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                                .filter(|e| {
                                    matches!(
                                        e,
                                        sui_types::object::bounded_visitor::Error::OutOfBudget
                                    )
                                })
                                .is_some() =>
                        {
                            tracing::warn!(
                                "Skipping struct with type {} because it was too large.",
                                tag
                            );
                            None
                        }
                        Err(err) => return Err(err),
                    }
                } else {
                    None
                };

                let mut object_wrapped_structs = BTreeMap::new();
                if let Some(move_struct) = move_struct {
                    parse_struct("$", move_struct, &mut object_wrapped_structs);
                }

                for (json_path, wrapped_struct) in object_wrapped_structs.iter() {
                    let entry = WrappedObjectEntry {
                        object_id: wrapped_struct.object_id.map(|id| id.to_string()),
                        root_object_id: object.id().to_string(),
                        root_object_version: object.version().value(),
                        checkpoint: checkpoint_seq,
                        epoch,
                        timestamp_ms,
                        json_path: json_path.to_string(),
                        struct_tag: wrapped_struct.struct_tag.clone().map(|tag| tag.to_string()),
                    };
                    wrapped_objects.push(entry);
                }
            }
        }

        Ok(wrapped_objects)
    }
}

#[async_trait]
impl Handler for WrappedObjectHandler {
    type Store = ObjectStore;
    type Batch = Vec<WrappedObjectEntry>;

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

        let first_checkpoint = batch.first().unwrap().checkpoint;
        let last_checkpoint = batch.last().unwrap().checkpoint;
        let epoch = batch.first().unwrap().epoch;

        use crate::parquet::ParquetWriter;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let mut writer: ParquetWriter =
            ParquetWriter::new(temp_dir.path(), FileType::WrappedObject, first_checkpoint)?;

        let rows: Vec<WrappedObjectEntry> = batch.to_vec();
        AnalyticsWriter::<WrappedObjectEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<WrappedObjectEntry>::flush(&mut writer, last_checkpoint + 1)?;

        let file_path = FileType::WrappedObject.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        let local_file = temp_dir
            .path()
            .join(FileType::WrappedObject.dir_prefix().as_ref())
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
