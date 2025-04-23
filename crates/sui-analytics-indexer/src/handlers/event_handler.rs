// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_core_types::annotated_value::MoveValue;
use std::sync::Arc;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;

use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use crate::handlers::AnalyticsHandler;
use crate::package_store::{LocalDBPackageStore, PackageCache};
use crate::tables::EventEntry;
use crate::FileType;
use sui_json_rpc_types::type_and_fields_from_move_event_data;
use sui_package_resolver::Resolver;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEvents;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::CheckpointData;

#[derive(Clone)]
pub struct EventHandler {
    state: Arc<Mutex<State>>,
}

struct State {
    events: Vec<EventEntry>,
    package_store: LocalDBPackageStore,
    resolver: Arc<Resolver<PackageCache>>,
}

#[async_trait::async_trait]
impl Worker for EventHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.events` in the
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
                    events: Vec::new(),
                    package_store: package_store.clone(),
                    resolver: resolver.clone(),
                };

                // Update local package store
                for object in checkpoint_transaction.output_objects.iter() {
                    local_state.package_store.update(object)?;
                }

                if let Some(events) = &checkpoint_transaction.events {
                    handler
                        .process_events(
                            epoch,
                            checkpoint_seq,
                            checkpoint_transaction.transaction.digest(),
                            timestamp_ms,
                            events,
                            &mut local_state,
                        )
                        .await?;
                }

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _permit = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state.events.extend(local_state.events.into_iter());

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
impl AnalyticsHandler<EventEntry> for EventHandler {
    async fn read(&self) -> Result<Vec<EventEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.events))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Event)
    }

    fn name(&self) -> &str {
        "event"
    }
}

impl EventHandler {
    pub fn new(package_store: LocalDBPackageStore, resolver: Arc<Resolver<PackageCache>>) -> Self {
        let state = State {
            events: vec![],
            package_store: package_store.clone(),
            resolver,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }
    async fn process_events(
        &self,
        epoch: u64,
        checkpoint: u64,
        digest: &TransactionDigest,
        timestamp_ms: u64,
        events: &TransactionEvents,
        state: &mut State,
    ) -> Result<()> {
        for (idx, event) in events.data.iter().enumerate() {
            let Event {
                package_id,
                transaction_module,
                sender,
                type_,
                contents,
            } = event;
            let layout = state
                .resolver
                .type_layout(move_core_types::language_storage::TypeTag::Struct(
                    Box::new(type_.clone()),
                ))
                .await?;
            let move_value = MoveValue::simple_deserialize(contents, &layout)?;
            let (_, event_json) = type_and_fields_from_move_event_data(move_value)?;
            let entry = EventEntry {
                transaction_digest: digest.base58_encode(),
                event_index: idx as u64,
                checkpoint,
                epoch,
                timestamp_ms,
                sender: sender.to_string(),
                package: package_id.to_string(),
                module: transaction_module.to_string(),
                event_type: type_.to_string(),
                bcs: "".to_string(),
                bcs_length: contents.len() as u64,
                event_json: event_json.to_string(),
            };

            state.events.push(entry);
        }
        Ok(())
    }
}
