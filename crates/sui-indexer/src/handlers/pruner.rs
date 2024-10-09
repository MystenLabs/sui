// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::spawn_monitored_task;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use strum_macros;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::RetentionConfig;
use crate::errors::IndexerError;
use crate::models::watermarks::PrunableWatermark;
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
    TxRecipients,
    TxSenders,

    Checkpoints,
    PrunerCpWatermark,
}

impl PrunableTable {
    pub fn select_lower_bound(&self, cp: u64, tx: u64) -> u64 {
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
            PrunableTable::TxRecipients => tx,
            PrunableTable::TxSenders => tx,

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

    pub async fn start(&self, cancel: CancellationToken) -> IndexerResult<()> {
        let store_clone = self.store.clone();
        let retention_policies = self.retention_policies.clone();
        let cancel_clone = cancel.clone();
        spawn_monitored_task!(update_watermarks_lower_bounds_task(
            store_clone,
            retention_policies,
            cancel_clone
        ));

        while !cancel.is_cancelled() {
            let (watermarks, latest_db_timestamp) = self.store.get_watermarks().await?;
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

            for watermark in watermarks.iter() {
                let Some(watermark) =
                    PrunableWatermark::new(watermark.clone(), latest_db_timestamp)
                else {
                    continue;
                };

                tokio::time::sleep(Duration::from_millis(watermark.prune_delay(1000))).await;

                // Prune as an epoch-partitioned table
                if table_partitions.get(watermark.entity.as_ref()).is_some() {
                    let mut prune_start = watermark.pruner_lo();
                    while prune_start < watermark.epoch_lo {
                        if cancel.is_cancelled() {
                            info!("Pruner task cancelled.");
                            return Ok(());
                        }
                        self.partition_manager
                            .drop_table_partition(
                                watermark.entity.as_ref().to_string(),
                                prune_start,
                            )
                            .await?;
                        info!(
                            "Batch dropped table partition {} epoch {}",
                            watermark.entity, prune_start
                        );
                        prune_start += 1;

                        // Then need to update the `pruned_lo`
                        self.store
                            .update_watermark_latest_pruned(watermark.entity.clone(), prune_start)
                            .await?;
                    }
                } else {
                    // Dealing with an unpartitioned table
                    if watermark.is_prunable() {
                        match watermark.entity {
                            PrunableTable::ObjectsHistory
                            | PrunableTable::Transactions
                            | PrunableTable::Events => {}
                            PrunableTable::EventEmitPackage
                            | PrunableTable::EventEmitModule
                            | PrunableTable::EventSenders
                            | PrunableTable::EventStructInstantiation
                            | PrunableTable::EventStructModule
                            | PrunableTable::EventStructName
                            | PrunableTable::EventStructPackage => {
                                self.store
                                    .prune_event_indices_table(
                                        watermark.pruner_lo(),
                                        watermark.reader_lo - 1,
                                    )
                                    .await?;
                            }
                            PrunableTable::TxAffectedAddresses
                            | PrunableTable::TxAffectedObjects
                            | PrunableTable::TxCallsPkg
                            | PrunableTable::TxCallsMod
                            | PrunableTable::TxCallsFun
                            | PrunableTable::TxChangedObjects
                            | PrunableTable::TxDigests
                            | PrunableTable::TxInputObjects
                            | PrunableTable::TxKinds
                            | PrunableTable::TxRecipients
                            | PrunableTable::TxSenders => {
                                self.store
                                    .prune_tx_indices_table(
                                        watermark.pruner_lo(),
                                        watermark.reader_lo - 1,
                                    )
                                    .await?;
                            }
                            PrunableTable::Checkpoints => {
                                self.store
                                    .prune_cp_tx_table(
                                        watermark.pruner_lo(),
                                        watermark.reader_lo - 1,
                                    )
                                    .await?;
                            }
                            PrunableTable::PrunerCpWatermark => {
                                self.store
                                    .prune_cp_tx_table(
                                        watermark.pruner_lo(),
                                        watermark.reader_lo - 1,
                                    )
                                    .await?;
                            }
                        }
                        self.store
                            .update_watermark_latest_pruned(
                                watermark.entity.clone(),
                                watermark.reader_lo - 1,
                            )
                            .await?;
                    }
                }
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
    let (watermarks, latest_db_timestamp) = store.get_watermarks().await?;
    let mut lower_bound_updates = vec![];

    for watermark in watermarks.iter() {
        if cancel.is_cancelled() {
            info!("Pruner watermark lower bound update task cancelled.");
            return Ok(());
        }

        let Some(watermark) = PrunableWatermark::new(watermark.clone(), latest_db_timestamp) else {
            continue;
        };

        let Some(epochs_to_keep) = retention_policies.get(&watermark.entity) else {
            continue;
        };

        if watermark.epoch_lo + epochs_to_keep <= watermark.epoch_hi_inclusive {
            let new_epoch_lower_bound = watermark
                .epoch_hi_inclusive
                .saturating_sub(epochs_to_keep - 1);

            lower_bound_updates.push((watermark.entity, new_epoch_lower_bound));
        }
    }

    if !lower_bound_updates.is_empty() {
        store
            .update_watermarks_lower_bound(lower_bound_updates)
            .await?;
        info!("Finished updating lower bounds for watermarks");
    }

    Ok(())
}
