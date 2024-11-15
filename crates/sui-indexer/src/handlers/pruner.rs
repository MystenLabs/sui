// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::spawn_monitored_task;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use strum_macros;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::config::RetentionConfig;
use crate::errors::IndexerError;
use crate::store::pg_partition_manager::PgPartitionManager;
use crate::store::PgIndexerStore;
use crate::{metrics::IndexerMetrics, store::IndexerStore, types::IndexerResult};

pub struct Pruner {
    pub store: PgIndexerStore,
    pub partition_manager: PgPartitionManager,
    // TODO: (wlmyng) - we can remove this when pruner logic is updated to use `retention_policies`.
    pub epochs_to_keep: u64,
    pub retention_policies: HashMap<PrunableTable, u64>,
    pub metrics: IndexerMetrics,
}

/// Enum representing tables that the pruner is allowed to prune. This corresponds to table names in
/// the database, and should be used in lieu of string literals. This enum is also meant to
/// facilitate the process of determining which unit (epoch, cp, or tx) should be used for the
/// table's range. Pruner will ignore any table that is not listed here.
#[derive(
    Debug,
    Eq,
    PartialEq,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::EnumIter,
    strum_macros::AsRefStr,
    Hash,
    Serialize,
    Deserialize,
    Clone,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PrunableTable {
    ObjectsHistory,
    Transactions,
    Events,

    EventEmitPackage,
    EventEmitModule,
    EventSenders,
    EventStructInstantiation,
    EventStructModule,
    EventStructName,
    EventStructPackage,

    TxAffectedAddresses,
    TxAffectedObjects,
    TxCallsPkg,
    TxCallsMod,
    TxCallsFun,
    TxChangedObjects,
    TxDigests,
    TxInputObjects,
    TxKinds,

    Checkpoints,
    PrunerCpWatermark,
}

impl PrunableTable {
    pub fn select_reader_lo(&self, cp: u64, tx: u64) -> u64 {
        match self {
            PrunableTable::ObjectsHistory => cp,
            PrunableTable::Transactions => tx,
            PrunableTable::Events => tx,

            PrunableTable::EventEmitPackage => tx,
            PrunableTable::EventEmitModule => tx,
            PrunableTable::EventSenders => tx,
            PrunableTable::EventStructInstantiation => tx,
            PrunableTable::EventStructModule => tx,
            PrunableTable::EventStructName => tx,
            PrunableTable::EventStructPackage => tx,

            PrunableTable::TxAffectedAddresses => tx,
            PrunableTable::TxAffectedObjects => tx,
            PrunableTable::TxCallsPkg => tx,
            PrunableTable::TxCallsMod => tx,
            PrunableTable::TxCallsFun => tx,
            PrunableTable::TxChangedObjects => tx,
            PrunableTable::TxDigests => tx,
            PrunableTable::TxInputObjects => tx,
            PrunableTable::TxKinds => tx,

            PrunableTable::Checkpoints => cp,
            PrunableTable::PrunerCpWatermark => cp,
        }
    }
}

impl Pruner {
    /// Instantiates a pruner with default retention and overrides. Pruner will finalize the
    /// retention policies so there is a value for every prunable table.
    pub fn new(
        store: PgIndexerStore,
        retention_config: RetentionConfig,
        metrics: IndexerMetrics,
    ) -> Result<Self, IndexerError> {
        let partition_manager = PgPartitionManager::new(store.pool())?;
        let epochs_to_keep = retention_config.epochs_to_keep;
        let retention_policies = retention_config.retention_policies();

        Ok(Self {
            store,
            epochs_to_keep,
            partition_manager,
            retention_policies,
            metrics,
        })
    }

    /// Given a table name, return the number of epochs to keep for that table. Return `None` if the
    /// table is not prunable.
    fn table_retention(&self, table_name: &str) -> Option<u64> {
        if let Ok(variant) = table_name.parse::<PrunableTable>() {
            self.retention_policies.get(&variant).copied()
        } else {
            None
        }
    }

    pub async fn start(&self, cancel: CancellationToken) -> IndexerResult<()> {
        let store_clone = self.store.clone();
        let retention_policies = self.retention_policies.clone();
        let cancel_clone = cancel.clone();
        spawn_monitored_task!(update_watermarks_lower_bounds_task(
            store_clone,
            retention_policies,
            cancel_clone
        ));

        let mut last_seen_max_epoch = 0;
        // The first epoch that has not yet been pruned.
        let mut next_prune_epoch = None;
        while !cancel.is_cancelled() {
            let (min_epoch, max_epoch) = self.store.get_available_epoch_range().await?;
            if max_epoch == last_seen_max_epoch {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            last_seen_max_epoch = max_epoch;

            // Not all partitioned tables are epoch-partitioned, so we need to filter them out.
            let table_partitions: HashMap<_, _> = self
                .partition_manager
                .get_table_partitions()
                .await?
                .into_iter()
                .filter(|(table_name, _)| {
                    self.partition_manager
                        .get_strategy(table_name)
                        .is_epoch_partitioned()
                })
                .collect();

            for (table_name, (min_partition, max_partition)) in &table_partitions {
                if let Some(epochs_to_keep) = self.table_retention(table_name) {
                    if last_seen_max_epoch != *max_partition {
                        error!(
                            "Epochs are out of sync for table {}: max_epoch={}, max_partition={}",
                            table_name, last_seen_max_epoch, max_partition
                        );
                    }

                    for epoch in
                        *min_partition..last_seen_max_epoch.saturating_sub(epochs_to_keep - 1)
                    {
                        if cancel.is_cancelled() {
                            info!("Pruner task cancelled.");
                            return Ok(());
                        }
                        self.partition_manager
                            .drop_table_partition(table_name.clone(), epoch)
                            .await?;
                        info!(
                            "Batch dropped table partition {} epoch {}",
                            table_name, epoch
                        );
                    }
                }
            }

            // TODO: (wlmyng) Once we have the watermarks table, we can iterate through each row
            // returned from `watermarks`, look it up against `retention_policies`, and process them
            // independently. This also means that pruning overrides will only apply for
            // epoch-partitioned tables right now.
            let prune_to_epoch = last_seen_max_epoch.saturating_sub(self.epochs_to_keep - 1);
            let prune_start_epoch = next_prune_epoch.unwrap_or(min_epoch);
            for epoch in prune_start_epoch..prune_to_epoch {
                if cancel.is_cancelled() {
                    info!("Pruner task cancelled.");
                    return Ok(());
                }
                info!("Pruning epoch {}", epoch);
                if let Err(err) = self.store.prune_epoch(epoch).await {
                    error!("Failed to prune epoch {}: {}", epoch, err);
                    break;
                };
                self.metrics.last_pruned_epoch.set(epoch as i64);
                info!("Pruned epoch {}", epoch);
                next_prune_epoch = Some(epoch + 1);
            }
        }
        info!("Pruner task cancelled.");
        Ok(())
    }
}

/// Task to periodically query the `watermarks` table and update the lower bounds for all watermarks
/// if the entry exceeds epoch-level retention policy.
async fn update_watermarks_lower_bounds_task(
    store: PgIndexerStore,
    retention_policies: HashMap<PrunableTable, u64>,
    cancel: CancellationToken,
) -> IndexerResult<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Pruner watermark lower bound update task cancelled.");
                return Ok(());
            }
            _ = interval.tick() => {
                update_watermarks_lower_bounds(&store, &retention_policies, &cancel).await?;
            }
        }
    }
}

/// Fetches all entries from the `watermarks` table, and updates the `reader_lo` for each entry if
/// its epoch range exceeds the respective retention policy.
async fn update_watermarks_lower_bounds(
    store: &PgIndexerStore,
    retention_policies: &HashMap<PrunableTable, u64>,
    cancel: &CancellationToken,
) -> IndexerResult<()> {
    let (watermarks, _) = store.get_watermarks().await?;
    let mut lower_bound_updates = vec![];

    for watermark in watermarks.iter() {
        if cancel.is_cancelled() {
            info!("Pruner watermark lower bound update task cancelled.");
            return Ok(());
        }

        let Some(prunable_table) = watermark.entity() else {
            continue;
        };

        let Some(epochs_to_keep) = retention_policies.get(&prunable_table) else {
            error!(
                "No retention policy found for prunable table {}",
                prunable_table
            );
            continue;
        };

        if let Some(new_epoch_lo) = watermark.new_epoch_lo(*epochs_to_keep) {
            lower_bound_updates.push((prunable_table, new_epoch_lo));
        };
    }

    if !lower_bound_updates.is_empty() {
        store
            .update_watermarks_lower_bound(lower_bound_updates)
            .await?;
        info!("Finished updating lower bounds for watermarks");
    }

    Ok(())
}
