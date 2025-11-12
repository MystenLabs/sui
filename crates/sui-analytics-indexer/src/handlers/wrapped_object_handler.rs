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

use crate::handlers::{get_move_struct, parse_struct};
use crate::package_store::PackageCache;
use crate::parquet::ParquetBatch;
use crate::tables::WrappedObjectEntry;
use crate::{FileType, PipelineConfig};

pub struct WrappedObjectBatch {
    pub inner: ParquetBatch<WrappedObjectEntry>,
}

impl Default for WrappedObjectBatch {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(FileType::WrappedObject, 0)
                .expect("Failed to create ParquetBatch"),
        }
    }
}

pub struct WrappedObjectHandler {
    package_cache: Arc<PackageCache>,
    config: PipelineConfig,
}

impl WrappedObjectHandler {
    pub fn new(package_cache: Arc<PackageCache>, config: PipelineConfig) -> Self {
        Self {
            package_cache,
            config,
        }
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
    type Batch = WrappedObjectBatch;


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

        batch.inner.set_epoch(first.epoch);
        batch.inner.update_last_checkpoint(first.checkpoint);

        // Write first value and remaining values
        if let Err(e) = batch
            .inner
            .write_rows(std::iter::once(first).chain(values.by_ref()))
        {
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
        let Some(file_path) = batch.inner.current_file_path() else {
            return Ok(0);
        };

        let row_count = batch.inner.row_count()?;
        let file_bytes = tokio::fs::read(file_path).await?;
        let object_path = batch.inner.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}
