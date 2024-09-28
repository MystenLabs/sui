// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::time::Duration;

use mysten_metrics::spawn_monitored_task;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::errors::IndexerError;
use crate::models::watermarks::{Watermark, WatermarkEntity, WatermarkRead};
use crate::store::pg_partition_manager::PgPartitionManager;
use crate::store::PgIndexerStore;
use crate::{metrics::IndexerMetrics, store::IndexerStore, types::IndexerResult};

/// The primary purpose of objects_history is to serve consistency query.
/// A short retention is sufficient.
const OBJECTS_HISTORY_EPOCHS_TO_KEEP: u64 = 2;

pub struct Pruner {
    pub store: PgIndexerStore,
    pub partition_manager: PgPartitionManager,
    pub epochs_to_keep: u64,
    pub metrics: IndexerMetrics,
}

impl Pruner {
    pub fn new(
        store: PgIndexerStore,
        epochs_to_keep: u64,
        metrics: IndexerMetrics,
    ) -> Result<Self, IndexerError> {
        let partition_manager = PgPartitionManager::new(store.pool())?;
        Ok(Self {
            store,
            partition_manager,
            epochs_to_keep,
            metrics,
        })
    }

    pub async fn start(&self, cancel: CancellationToken) -> IndexerResult<()> {
        // Spawn a separate task to continuously update the watermarks for the reader. We can't
        // reliably update watermarks while we're in the middle of a pruning operation. This is
        // because the pruning operation itself may take a considerable amount of time, during which
        // the system state could change significantly. Pruner prunes on the main thread to limit
        // concurrency.
        let store_clone = self.store.clone();
        let epochs_to_keep = self.epochs_to_keep;
        let cancel_clone = cancel.clone();
        spawn_monitored_task!(update_watermarks_lower_bounds_task(
            store_clone,
            epochs_to_keep,
            cancel_clone
        ));

        // I think the main thread can just continuously fetch watermarks
        // iterate through and create a future for each thing to prune
        // and then await all of them?
        // the separate task will continue to update watermarks

        // Similarly, handle epoch-partitioned tables in a separate task, so that the main thread
        // can focus on the slowest pruning operation.
        let store_clone = self.store.clone();
        let partition_manager = self.partition_manager.clone();
        let cancel_clone = cancel.clone();
        spawn_monitored_task!(prune_epoch_partitioned_tables_task(
            store_clone,
            partition_manager,
            cancel_clone.clone(),
        ));

        while !cancel.is_cancelled() {
            let watermarks = self.store.get_watermarks().await?;

            for (key, watermark) in watermarks.iter() {
                if cancel.is_cancelled() {
                    info!("Pruner task cancelled.");
                    return Ok(());
                }

                if should_prune(watermark) {
                    // reader_lo - 1 because we don't want to prune the readable lower bound
                    let inclusive_upper_bound = watermark.reader_lo().saturating_sub(1);

                    match key {
                        WatermarkEntity::Checkpoints => {
                            self.store
                                .prune_checkpoints_table(
                                    watermark.pruner_lo(),
                                    inclusive_upper_bound,
                                )
                                .await?;
                            self.store
                                .update_watermarks(vec![Watermark::new_lower_bound_tail(
                                    WatermarkEntity::Checkpoints,
                                    watermark.pruner_lo(),
                                )])
                                .await?;
                        }
                        WatermarkEntity::EvLookup => {
                            self.store
                                .prune_event_indices_table(
                                    watermark.pruner_lo(),
                                    inclusive_upper_bound,
                                )
                                .await?;
                            self.store
                                .update_watermarks(vec![Watermark::new_lower_bound_tail(
                                    WatermarkEntity::EvLookup,
                                    inclusive_upper_bound,
                                )])
                                .await?;
                        }
                        WatermarkEntity::TxLookup => {
                            self.store
                                .prune_tx_indices_table(
                                    watermark.pruner_lo(),
                                    inclusive_upper_bound,
                                )
                                .await?;
                            self.store
                                .update_watermarks(vec![Watermark::new_lower_bound_tail(
                                    WatermarkEntity::TxLookup,
                                    inclusive_upper_bound,
                                )])
                                .await?;
                        }
                        _ => {}
                    }
                }
            }
        }
        info!("Pruner task cancelled.");
        Ok(())
    }
}

/// Check if the lowest epoch of unpruned data exceeds the retention policy.
fn should_update_watermark(watermark: &WatermarkRead, epochs_to_keep: u64) -> bool {
    watermark.epoch_lo + epochs_to_keep <= watermark.epoch_hi
}

/// Determine if pruning should occur based on the current watermark state. When an entity's `hi`
/// and `lo` (`reader_lo`) watermarks are updated to reflect the latest data range, if the
/// `reader_lo` value is at least 1 greater than `pruned_lo`, then the pruner should prune. When
/// `lo` is updated, and `pruned_lo` is still unset, that also means the pruner should start
/// pruning.
fn should_prune(watermark: &WatermarkRead) -> bool {
    match watermark.pruned_lo {
        None => watermark.reader_lo() > 0,
        Some(pruned_lo) => watermark.reader_lo() > pruned_lo + 1,
    }
}

/// Pruner waits for some time before pruning to ensure that in-flight reads complete or timeout
/// before the underlying data is pruned. While waiting, it is possible that the valid data range
/// further shifts into the future. Callers of this function can still proceed with pruning because
/// the data to prune must be much staler than any data accessible by readers.
async fn wait_for_prune_delay(
    watermark: &WatermarkRead,
    cancel: &CancellationToken,
    delay_amount: i64,
) -> IndexerResult<()> {
    let current_time = chrono::Utc::now().timestamp_millis();
    let delay = (watermark.timestamp_ms + delay_amount - current_time).max(0) as u64;

    if delay > 0 {
        info!("Waiting for {}ms before pruning", delay);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(delay)) => Ok(()),
            _ = cancel.cancelled() => {
                info!("Pruning cancelled during delay");
                Ok(())
            }
        }
    } else {
        Ok(())
    }
}

/// Fetches all entries from the `watermarks` table, and updates the lower bounds for all watermarks
/// if the entry's epoch range exceeds the respective retention policy.
async fn update_watermarks_lower_bounds(
    store: &PgIndexerStore,
    epochs_to_keep: u64,
) -> IndexerResult<()> {
    let watermarks = store.get_watermarks().await?;
    let mut lower_bound_updates = vec![];

    for (key, value) in watermarks.iter() {
        // Determine the retention policy for the entry, and calculate the inclusive lower bound for
        // the reader.

        // TODO: (wlmyng) actually introduce the mapping
        let epochs_to_keep = if key.as_str() == "objects_history" {
            OBJECTS_HISTORY_EPOCHS_TO_KEEP
        } else {
            epochs_to_keep
        };

        // We should update the watermarks and prepare for pruning
        if should_update_watermark(value, epochs_to_keep) {
            let new_inclusive_epoch_lower_bound = value.epoch_hi.saturating_sub(epochs_to_keep - 1);
            let (cp, tx) = store
                .get_min_cp_and_tx_for_epoch(new_inclusive_epoch_lower_bound)
                .await?;
            let new_lo = value.map_to_bound(cp, tx);
            lower_bound_updates.push(Watermark::lower_bound(
                *key,
                new_inclusive_epoch_lower_bound,
                new_lo,
            ));
        }
    }

    if !lower_bound_updates.is_empty() {
        store.update_watermarks(lower_bound_updates).await?;
        info!("Finished updating lower bounds for watermarks");
    }

    Ok(())
}

/// Task to periodically query the `watermarks` table and update the lower bounds for all watermarks
/// if the entry exceeds epoch-level retention policy.
async fn update_watermarks_lower_bounds_task(
    store: PgIndexerStore,
    epochs_to_keep: u64,
    cancel: CancellationToken,
) -> IndexerResult<()> {
    loop {
        if cancel.is_cancelled() {
            info!("Pruner watermark lower bound update task cancelled.");
            return Ok(());
        }

        update_watermarks_lower_bounds(&store, epochs_to_keep).await?;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// A task that queries `watermarks` table for the source of truth for the epoch range, and drops
/// older partitions up to but not including `watermark.epoch_lo`.
async fn prune_epoch_partitioned_tables_task(
    store: PgIndexerStore,
    partition_manager: PgPartitionManager,
    cancel: CancellationToken,
) -> IndexerResult<()> {
    loop {
        if cancel.is_cancelled() {
            info!("Pruner prune_epoch_partitioned_tables_task cancelled.");
            return Ok(());
        }

        let watermarks = store.get_watermarks().await?;

        // Not all partitioned tables are epoch-partitioned, so we need to filter them out.
        let table_partitions: HashMap<_, _> = partition_manager
            .get_table_partitions()
            .await?
            .into_iter()
            .filter(|(table_name, _)| {
                partition_manager
                    .get_strategy(table_name)
                    .is_epoch_partitioned()
            })
            .collect();

        // `watermarks` table is the source of truth for the epoch range. The partitions to drop are
        // `[watermark.pruner_lo(), watermark.epoch_lo)`
        for (table_name, (_, _)) in &table_partitions {
            let Some(lookup) = WatermarkEntity::from_str(table_name) else {
                // TODO: (wlmyng) handle this error
                println!(
                    "could not convert table name to WatermarkEntity: {}",
                    table_name
                );
                continue;
            };

            let Some(entry) = watermarks.get(&lookup) else {
                println!("coudl not find entity in watermarks: {:?}", lookup);
                continue;
            };

            wait_for_prune_delay(&entry, &cancel, 1000).await?;

            for epoch in entry.pruner_lo()..entry.epoch_lo {
                if cancel.is_cancelled() {
                    info!("Pruner prune_epoch_partitioned_tables_task task cancelled.");
                    return Ok(());
                }
                partition_manager
                    .drop_table_partition(table_name.clone(), epoch)
                    .await?;

                store
                    .update_watermarks(vec![Watermark::new_lower_bound_tail(lookup, epoch)])
                    .await?;

                info!("Dropped table partition {} epoch {}", table_name, epoch);
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
