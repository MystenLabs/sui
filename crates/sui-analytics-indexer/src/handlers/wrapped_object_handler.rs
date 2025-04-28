// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Arc;
use futures::{stream, StreamExt};
use sui_data_ingestion_core::Worker;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::sync::Mutex;

use sui_types::full_checkpoint_content::CheckpointData;

use crate::handlers::{get_move_struct, parse_struct, AnalyticsHandler};
use crate::AnalyticsMetrics;

use crate::package_store::PackageCache;
use crate::tables::WrappedObjectEntry;
use crate::FileType;

const NAME: &str = "wrapped_object";
#[derive(Clone)]
pub struct WrappedObjectHandler {
    state: Arc<Mutex<Vec<WrappedObjectEntry>>>,
    metrics: AnalyticsMetrics,
    package_cache: Arc<PackageCache>,
}

#[async_trait::async_trait]
impl Worker for WrappedObjectHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        // Update package cache first (serial operation)
        for checkpoint_transaction in &checkpoint_data.transactions {
            for object in checkpoint_transaction.output_objects.iter() {
                self.package_cache.update(object)?;
            }
        }

        // Process transactions in parallel using buffered stream for ordered execution
        let txn_len = checkpoint_data.transactions.len();
        let mut entries = Vec::new();
        
        let mut stream = stream::iter(0..txn_len)
            .map(|idx| {
                let cp = checkpoint_data.clone();
                let handler = self.clone();
                tokio::spawn(async move { 
                    handle_tx(idx, &cp, &handler).await
                })
            })
            .buffered(num_cpus::get() * 4);

        while let Some(join_res) = stream.next().await {
            match join_res {
                Ok(Ok(tx_entries)) => {
                    entries.extend(tx_entries);
                }
                Ok(Err(e)) => {
                    // Task executed but application logic returned an error
                    return Err(e);
                }
                Err(e) => {
                    // Task panicked or was cancelled
                    return Err(anyhow::anyhow!("Task join error: {}", e));
                }
            }
        }

        // If end of epoch, evict package store
        if checkpoint_data
            .checkpoint_summary
            .end_of_epoch_data
            .is_some()
        {
            self.package_cache
                .resolver
                .package_store()
                .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
        }

        // Store results
        *self.state.lock().await = entries;

        Ok(())
    }
}

/// Private per-tx helper for processing individual transactions
async fn handle_tx(
    tx_idx: usize, 
    checkpoint: &CheckpointData,
    handler: &WrappedObjectHandler
) -> Result<Vec<WrappedObjectEntry>> {
    let transaction = &checkpoint.transactions[tx_idx];
    let epoch = checkpoint.checkpoint_summary.epoch;
    let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
    let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

    let mut wrapped_objects = Vec::new();
    for object in transaction.output_objects.iter() {
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            match get_move_struct(&tag, contents, &handler.package_cache.resolver).await {
                Ok(move_struct) => Some(move_struct),
                Err(err)
                    if err
                        .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                        .filter(|e| {
                            matches!(e, sui_types::object::bounded_visitor::Error::OutOfBudget)
                        })
                        .is_some() =>
                {
                    handler.metrics
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
            wrapped_objects.push(entry);
        }
    }

    Ok(wrapped_objects)
}

#[async_trait::async_trait]
impl AnalyticsHandler<WrappedObjectEntry> for WrappedObjectHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = WrappedObjectEntry>>> {
        let mut state = self.state.lock().await;
        let entries = std::mem::take(&mut *state);

        // Return all entries
        Ok(Box::new(entries.into_iter()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::WrappedObject)
    }

    fn name(&self) -> &'static str {
        NAME
    }
}

impl WrappedObjectHandler {
    pub fn new(package_cache: Arc<PackageCache>, metrics: AnalyticsMetrics) -> Self {
        WrappedObjectHandler {
            state: Arc::new(Mutex::new(Vec::new())),
            metrics,
            package_cache,
        }
    }
}
