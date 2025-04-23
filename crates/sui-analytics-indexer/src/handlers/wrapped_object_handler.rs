// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use sui_package_resolver::Resolver;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::object::Object;

use crate::handlers::{get_move_struct, parse_struct, AnalyticsHandler};
use crate::AnalyticsMetrics;

use crate::package_store::{LocalDBPackageStore, PackageCache};
use crate::tables::WrappedObjectEntry;
use crate::FileType;

#[derive(Clone)]
pub struct WrappedObjectHandler {
    state: Arc<Mutex<State>>,
    metrics: AnalyticsMetrics,
}

struct State {
    wrapped_objects: Vec<WrappedObjectEntry>,
    package_store: LocalDBPackageStore,
    resolver: Arc<Resolver<PackageCache>>,
}

#[async_trait::async_trait]
impl Worker for WrappedObjectHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.wrapped_objects` in the
        // same order as `checkpoint_transactions`, while allowing *everything*
        // else to run in parallel.
        // ──────────────────────────────────────────────────────────────────────────
        let txn_count = checkpoint_transactions.len();
        let semaphores: Vec<_> = (0..txn_count)
            .map(|i| {
                if i == 0 {
                    Arc::new(Semaphore::new(1)) // first txn proceeds immediately
                } else {
                    Arc::new(Semaphore::new(0))
                }
            })
            .collect();

        let mut handles: Vec<JoinHandle<Result<()>>> = Vec::with_capacity(txn_count);

        // Clone the package store so we can mutate it freely in parallel.
        let (package_store, resolver) = {
            let guard = self.state.lock().await;
            (guard.package_store.clone(), guard.resolver.clone())
        };

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().cloned().enumerate() {
            let handler = self.clone();
            let start_sem = semaphores[idx].clone();
            let next_sem = semaphores.get(idx + 1).cloned();

            // Snapshot any data we need from the summary (Copy types, cheap).
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;
            let end_of_epoch = checkpoint_summary.end_of_epoch_data.is_some();
            let package_store = package_store.clone();
            let resolver = resolver.clone();

            let handle = tokio::spawn(async move {
                // ───── 1. Heavy work off‑mutex ───────────────────────────────────

                let mut local_state = State {
                    wrapped_objects: Vec::new(),
                    package_store: package_store.clone(),
                    resolver: resolver.clone(),
                };

                // Update local package store
                for object in checkpoint_transaction.output_objects.iter() {
                    local_state.package_store.update(object)?;
                }

                handler
                    .process_transaction(
                        epoch,
                        checkpoint_seq,
                        timestamp_ms,
                        &checkpoint_transaction,
                        &mut local_state,
                    )
                    .await?;

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _permit = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state
                        .wrapped_objects
                        .extend(local_state.wrapped_objects.into_iter());

                    if end_of_epoch {
                        shared_state
                            .resolver
                            .package_store()
                            .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
                    }
                }

                // Signal the next task.
                if let Some(next) = next_sem {
                    next.add_permits(1);
                }

                Ok(())
            });

            handles.push(handle);
        }

        // Propagate any error.
        for h in handles {
            h.await??;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<WrappedObjectEntry> for WrappedObjectHandler {
    async fn read(&self) -> Result<Vec<WrappedObjectEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.wrapped_objects))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::WrappedObject)
    }

    fn name(&self) -> &str {
        "wrapped_object"
    }
}

impl WrappedObjectHandler {
    pub fn new(package_store: LocalDBPackageStore, metrics: AnalyticsMetrics, resolver: Option<Arc<Resolver<PackageCache>>>) -> Self {
        let resolver = resolver.unwrap_or_else(|| {
            Arc::new(Resolver::new(PackageCache::new(package_store.clone())))
        });
        
        let state = Arc::new(Mutex::new(State {
            wrapped_objects: vec![],
            package_store: package_store.clone(),
            resolver,
        }));
        WrappedObjectHandler { state, metrics }
    }
    async fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        state: &mut State,
    ) -> Result<()> {
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_object(epoch, checkpoint, timestamp_ms, object, state)
                .await?;
        }
        Ok(())
    }

    async fn process_object(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        state: &mut State,
    ) -> Result<()> {
        let move_struct = if let Some((tag, contents)) = object
            .struct_tag()
            .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
        {
            match get_move_struct(&tag, contents, &state.resolver).await {
                Ok(move_struct) => Some(move_struct),
                Err(err)
                    if err
                        .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                        .filter(|e| {
                            matches!(e, sui_types::object::bounded_visitor::Error::OutOfBudget)
                        })
                        .is_some() =>
                {
                    self.metrics
                        .total_too_large_to_deserialize
                        .with_label_values(&[self.name()])
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
        let mut wrapped_structs = BTreeMap::new();
        if let Some(move_struct) = move_struct {
            parse_struct("$", move_struct, &mut wrapped_structs);
        }
        for (json_path, wrapped_struct) in wrapped_structs.iter() {
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
            state.wrapped_objects.push(entry);
        }
        Ok(())
    }
}
