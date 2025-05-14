// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;

use crate::handlers::{get_move_struct, parse_struct, AnalyticsHandler};
use crate::{AnalyticsMetrics, FileType};

use crate::package_store::PackageCache;
use crate::tables::WrappedObjectEntry;

use super::wait_for_cache;

const NAME: &str = "wrapped_object";
#[derive(Clone)]
pub struct WrappedObjectHandler {
    metrics: AnalyticsMetrics,
    package_cache: Arc<PackageCache>,
}

impl WrappedObjectHandler {
    pub fn new(package_cache: Arc<PackageCache>, metrics: AnalyticsMetrics) -> Self {
        WrappedObjectHandler {
            metrics,
            package_cache,
        }
    }

    async fn process_transactions(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<WrappedObjectEntry>> {
        let txn_len = checkpoint_data.transactions.len();
        let mut entries = Vec::new();

        for idx in 0..txn_len {
            let transaction = &checkpoint_data.transactions[idx];

            // Update package cache with output objects
            for object in transaction.output_objects.iter() {
                self.package_cache.update(object)?;
            }

            let epoch = checkpoint_data.checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_data.checkpoint_summary.timestamp_ms;

            for object in transaction.output_objects.iter() {
                let move_struct = if let Some((tag, contents)) = object
                    .struct_tag()
                    .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
                {
                    match get_move_struct(&tag, contents, &self.package_cache.resolver).await {
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
                            self.metrics
                                .total_too_large_to_deserialize
                                .with_label_values(&[NAME])
                                .inc();
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
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<WrappedObjectEntry> for WrappedObjectHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<WrappedObjectEntry>> {
        wait_for_cache(checkpoint_data, &self.package_cache).await;
        self.process_transactions(checkpoint_data).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::WrappedObject)
    }

    fn name(&self) -> &'static str {
        NAME
    }
}
