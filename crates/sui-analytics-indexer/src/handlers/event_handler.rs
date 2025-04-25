// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use futures::future::try_join_all;
use move_core_types::annotated_value::MoveValue;
use sui_data_ingestion_core::Worker;
use sui_json_rpc_types::type_and_fields_from_move_event_data;
use sui_types::{
    digests::TransactionDigest, effects::TransactionEvents, event::Event,
    full_checkpoint_content::CheckpointData, SYSTEM_PACKAGE_ADDRESSES,
};
use tokio::sync::{Mutex, Semaphore};

use crate::{
    handlers::AnalyticsHandler, package_store::PackageCache, tables::EventEntry, FileType,
};

pub struct EventHandler {
    state: Mutex<State>,
    package_cache: Arc<PackageCache>,
}

struct State {
    events: Vec<EventEntry>,
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

        // Nothing to do
        if checkpoint_transactions.is_empty() {
            return Ok(());
        }

        //----------------------------------------------------------------------
        // Concurrency scaffolding – semaphore-chain (one per transaction)
        //----------------------------------------------------------------------
        let n = checkpoint_transactions.len();
        let sems: Vec<Arc<Semaphore>> = (0..n).map(|_| Arc::new(Semaphore::new(0))).collect();
        sems[0].add_permits(1); // first task may flush immediately

        // Clone so we can move into async blocks
        let txs = checkpoint_transactions.clone();
        let mut futs = Vec::with_capacity(n);

        for (idx, tx) in txs.into_iter().enumerate() {
            let sem_curr = sems[idx].clone();
            let sem_next = sems.get(idx + 1).cloned();
            let pkg_cache = self.package_cache.clone();
            let state_mutex = &self.state;
            let epoch = checkpoint_summary.epoch;
            let checkpoint = checkpoint_summary.sequence_number;
            let ts_ms = checkpoint_summary.timestamp_ms;
            let end_of_epoch = checkpoint_summary.end_of_epoch_data.is_some();

            futs.push(async move {
                // 1. Package-cache updates can run fully in parallel.
                for obj in tx.output_objects.iter() {
                    pkg_cache.update(obj)?;
                }

                // 2. Build events locally.
                let mut local_events = Vec::new();
                if let Some(events) = &tx.events {
                    Self::collect_events(
                        &pkg_cache,
                        epoch,
                        checkpoint,
                        tx.transaction.digest(),
                        ts_ms,
                        events,
                        &mut local_events,
                    )
                    .await?;
                }

                // 3. Wait our turn to flush into shared state.
                sem_curr.acquire().await.unwrap().forget();
                {
                    let mut state = state_mutex.lock().await;
                    state.events.extend(local_events);
                }

                // 4. Evict system packages once per checkpoint (idempotent).
                if end_of_epoch {
                    pkg_cache
                        .resolver
                        .package_store()
                        .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
                }

                // 5. Unblock the next task in line.
                if let Some(next) = sem_next {
                    next.add_permits(1);
                }

                Ok::<(), anyhow::Error>(())
            });
        }

        try_join_all(futs).await?;
        Ok(())
    }
}

impl EventHandler {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self {
            state: Mutex::new(State { events: Vec::new() }),
            package_cache,
        }
    }

    // ---------------------------------------------------------------------
    // Helper that materialises all events from one transaction.
    // ---------------------------------------------------------------------
    async fn collect_events(
        package_cache: &PackageCache,
        epoch: u64,
        checkpoint: u64,
        digest: &TransactionDigest,
        timestamp_ms: u64,
        events: &TransactionEvents,
        out: &mut Vec<EventEntry>,
    ) -> Result<()> {
        for (idx, ev) in events.data.iter().enumerate() {
            let Event {
                package_id,
                transaction_module,
                sender,
                type_,
                contents,
            } = ev;

            let layout = package_cache
                .resolver
                .type_layout(move_core_types::language_storage::TypeTag::Struct(
                    Box::new(type_.clone()),
                ))
                .await?;

            let mv = MoveValue::simple_deserialize(contents, &layout)?;
            let (_, json) = type_and_fields_from_move_event_data(mv)?;

            out.push(EventEntry {
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
                event_json: json.to_string(),
            });
        }
        Ok(())
    }
}

// -------------------------------------------------------------------------
// AnalyticsHandler impl – unchanged behaviour
// -------------------------------------------------------------------------
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
