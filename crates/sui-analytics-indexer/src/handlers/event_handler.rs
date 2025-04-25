// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use move_core_types::annotated_value::MoveValue;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;

use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use crate::handlers::AnalyticsHandler;
use crate::package_store::PackageCache;
use crate::tables::EventEntry;
use crate::FileType;
use sui_json_rpc_types::type_and_fields_from_move_event_data;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEvents;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::CheckpointData;

// Context contains shared resources
struct Context {
    package_cache: Arc<PackageCache>,
}

pub struct EventHandler {
    state: Mutex<BTreeMap<usize, Vec<EventEntry>>>,
    context: Arc<Context>,
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

        // Update package cache first (still need to do this serially)
        for checkpoint_transaction in checkpoint_transactions {
            for object in checkpoint_transaction.output_objects.iter() {
                self.context.package_cache.update(object)?;
            }
        }

        // Create a channel to collect results
        let (tx, mut rx) =
            tokio::sync::mpsc::channel::<(usize, Vec<EventEntry>)>(checkpoint_transactions.len());

        // Process transactions in parallel
        let mut futures = Vec::new();

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().enumerate() {
            if let Some(events) = &checkpoint_transaction.events {
                let tx = tx.clone();
                let context = self.context.clone();
                let transaction = checkpoint_transaction.clone();
                let epoch = checkpoint_summary.epoch;
                let checkpoint_seq = checkpoint_summary.sequence_number;
                let timestamp_ms = checkpoint_summary.timestamp_ms;
                let events_clone = events.clone();

                // Spawn a task for each transaction
                let handle = tokio::spawn(async move {
                    match Self::process_events(
                        epoch,
                        checkpoint_seq,
                        transaction.transaction.digest(),
                        timestamp_ms,
                        &events_clone,
                        context,
                    )
                    .await
                    {
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
        while let Some((idx, events)) = rx.recv().await {
            state.insert(idx, events);
        }

        // If end of epoch, evict package store
        if checkpoint_summary.end_of_epoch_data.is_some() {
            self.context
                .package_cache
                .resolver
                .package_store()
                .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<EventEntry> for EventHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = EventEntry>>> {
        let mut state = self.state.lock().await;
        let events_map = std::mem::take(&mut *state);

        // Flatten the map into a single iterator in order by transaction index
        Ok(Box::new(events_map.into_values().flatten()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Event)
    }

    fn name(&self) -> &'static str {
        "event"
    }
}

impl EventHandler {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        let context = Arc::new(Context { package_cache });

        Self {
            state: Mutex::new(BTreeMap::new()),
            context,
        }
    }
    async fn process_events(
        epoch: u64,
        checkpoint: u64,
        digest: &TransactionDigest,
        timestamp_ms: u64,
        events: &TransactionEvents,
        context: Arc<Context>,
    ) -> Result<Vec<EventEntry>> {
        let mut entries = Vec::new();
        for (idx, event) in events.data.iter().enumerate() {
            let Event {
                package_id,
                transaction_module,
                sender,
                type_,
                contents,
            } = event;
            let layout = context
                .package_cache
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

            entries.push(entry);
        }
        Ok(entries)
    }
}
