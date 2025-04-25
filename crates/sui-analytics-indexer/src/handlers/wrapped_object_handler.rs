// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::sync::Mutex;

use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};

use crate::handlers::{get_move_struct, parse_struct, AnalyticsHandler};
use crate::AnalyticsMetrics;

use crate::package_store::PackageCache;
use crate::tables::WrappedObjectEntry;
use crate::FileType;

const NAME: &str = "wrapped_object";
pub struct WrappedObjectHandler {
    state: Mutex<BTreeMap<usize, Vec<WrappedObjectEntry>>>,
    metrics: AnalyticsMetrics,
    package_cache: Arc<PackageCache>,
}

#[async_trait::async_trait]
impl Worker for WrappedObjectHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let checkpoint_summary = &checkpoint_data.checkpoint_summary;
        let checkpoint_transactions = &checkpoint_data.transactions;

        // Update package cache for all objects first
        for checkpoint_transaction in checkpoint_transactions {
            for object in checkpoint_transaction.output_objects.iter() {
                self.package_cache.update(object)?;
            }
        }

        // Create a channel to collect results
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, Vec<WrappedObjectEntry>)>(
            checkpoint_transactions.len(),
        );

        // Process transactions in parallel
        let mut futures = Vec::new();

        for (idx, _checkpoint_transaction) in checkpoint_transactions.iter().enumerate() {
            let tx = tx.clone();
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;
            let package_cache = self.package_cache.clone();
            let metrics = self.metrics.clone();
            let checkpoint_data_clone = checkpoint_data.clone();

            // Spawn a task for each transaction
            let handle = tokio::spawn(async move {
                let transaction = &checkpoint_data_clone.transactions[idx];
                match Self::process_transaction(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    transaction,
                    &package_cache,
                    &metrics,
                ).await {
                    Ok(entries) => {
                        if !entries.is_empty() {
                            let _ = tx.send((idx, entries)).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error processing transaction at index {}: {}", idx, e);
                    }
                }
            });

            futures.push(handle);
        }

        // Drop the original sender so the channel can close when all tasks are done
        drop(tx);

        // Wait for all tasks to complete
        for handle in futures {
            if let Err(e) = handle.await {
                tracing::error!("Task panicked: {}", e);
            }
        }

        // Collect results into the state in order by transaction index
        let mut state = self.state.lock().await;
        while let Some((idx, wrapped_objects)) = rx.recv().await {
            state.insert(idx, wrapped_objects);
        }

        // Handle end of epoch eviction
        if checkpoint_summary.end_of_epoch_data.is_some() {
            self.package_cache
                .resolver
                .package_store()
                .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<WrappedObjectEntry> for WrappedObjectHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = WrappedObjectEntry>>> {
        let mut state = self.state.lock().await;
        let wrapped_objects_map = std::mem::take(&mut *state);

        // Flatten the map into a single iterator in order by transaction index
        Ok(Box::new(wrapped_objects_map.into_values().flatten()))
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
            state: Mutex::new(BTreeMap::new()),
            metrics,
            package_cache,
        }
    }
    
    async fn process_transaction(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        package_cache: &Arc<PackageCache>,
        metrics: &AnalyticsMetrics,
    ) -> Result<Vec<WrappedObjectEntry>> {
        let mut wrapped_objects = Vec::new();
        for object in checkpoint_transaction.output_objects.iter() {
            let move_struct = if let Some((tag, contents)) = object
                .struct_tag()
                .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
            {
                match get_move_struct(&tag, contents, &package_cache.resolver).await {
                    Ok(move_struct) => Some(move_struct),
                    Err(err)
                        if err
                            .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                            .filter(|e| {
                                matches!(e, sui_types::object::bounded_visitor::Error::OutOfBudget)
                            })
                            .is_some() =>
                    {
                        metrics
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
                    checkpoint,
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
}
