// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::query_dsl::methods::FilterDsl;
use diesel::ExpressionMethods;
use futures::future::join_all;
use mysten_metrics::spawn_monitored_task;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use strum::IntoEnumIterator;
use strum_macros;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::config::RetentionConfig;
use crate::errors::IndexerError;
use crate::execute_delete_range_query;
use crate::schema::{
    checkpoints, event_emit_module, event_emit_package, event_senders, event_struct_instantiation,
    event_struct_module, event_struct_name, event_struct_package, events, objects_history,
    transactions, tx_affected_addresses, tx_affected_objects, tx_calls_fun, tx_calls_mod,
    tx_calls_pkg, tx_changed_objects, tx_digests, tx_input_objects, tx_kinds,
};
use crate::store::pg_partition_manager::PgPartitionManager;
use crate::store::PgIndexerStore;
use crate::{metrics::IndexerMetrics, store::IndexerStore, types::IndexerResult};

const MAX_DELAY_MS: u64 = 10000;

pub struct Pruner {
    pub store: PgIndexerStore,
    pub partition_manager: PgPartitionManager,
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
    Copy,
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
}

struct TablePruner {
    table: PrunableTable,
    store: PgIndexerStore,
    partition_manager: PgPartitionManager,
    cancel: CancellationToken,
}

impl TablePruner {
    fn new(
        table: PrunableTable,
        store: PgIndexerStore,
        partition_manager: PgPartitionManager,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            table,
            store,
            partition_manager,
            cancel,
        }
    }

    async fn run(&mut self) -> IndexerResult<()> {
        loop {
            if self.cancel.is_cancelled() {
                info!("Pruner task cancelled.");
                return Ok(());
            }

            let (watermark, _) = self.store.get_watermark(self.table).await?;

            let Some(pruner_hi) = watermark.pruner_hi else {
                continue;
            };

            self.prune(0, pruner_hi as u64).await?;
        }
    }

    async fn prune(&self, prune_min: u64, prune_max: u64) -> IndexerResult<()> {
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

        if let Some((min_partition, _)) = table_partitions.get(self.table.as_ref()) {
            for epoch in *min_partition..=prune_max {
                self.partition_manager
                    .drop_table_partition(self.table.as_ref().to_string(), epoch)
                    .await?;
            }
            return Ok(());
        };

        let pool = self.store.pool();
        let mut conn = pool.get().await?;

        use diesel_async::RunQueryDsl;

        if let Err(err) = match self.table {
            PrunableTable::ObjectsHistory => execute_delete_range_query!(
                &mut conn,
                objects_history,
                checkpoint_sequence_number,
                prune_min,
                prune_max
            ),
            PrunableTable::Transactions => {
                execute_delete_range_query!(
                    &mut conn,
                    transactions,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::Events => {
                execute_delete_range_query!(
                    &mut conn,
                    events,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::EventEmitPackage => {
                execute_delete_range_query!(
                    &mut conn,
                    event_emit_package,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::EventEmitModule => {
                execute_delete_range_query!(
                    &mut conn,
                    event_emit_module,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::EventSenders => {
                execute_delete_range_query!(
                    &mut conn,
                    event_senders,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::EventStructInstantiation => execute_delete_range_query!(
                &mut conn,
                event_struct_instantiation,
                tx_sequence_number,
                prune_min,
                prune_max
            ),
            PrunableTable::EventStructModule => execute_delete_range_query!(
                &mut conn,
                event_struct_module,
                tx_sequence_number,
                prune_min,
                prune_max
            ),
            PrunableTable::EventStructName => {
                execute_delete_range_query!(
                    &mut conn,
                    event_struct_name,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::EventStructPackage => execute_delete_range_query!(
                &mut conn,
                event_struct_package,
                tx_sequence_number,
                prune_min,
                prune_max
            ),
            PrunableTable::TxAffectedAddresses => execute_delete_range_query!(
                &mut conn,
                tx_affected_addresses,
                tx_sequence_number,
                prune_min,
                prune_max
            ),
            PrunableTable::TxAffectedObjects => execute_delete_range_query!(
                &mut conn,
                tx_affected_objects,
                tx_sequence_number,
                prune_min,
                prune_max
            ),
            PrunableTable::TxCallsPkg => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_calls_pkg,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::TxCallsMod => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_calls_mod,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::TxCallsFun => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_calls_fun,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::TxChangedObjects => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_changed_objects,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::TxDigests => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_digests,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::TxInputObjects => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_input_objects,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::TxKinds => {
                execute_delete_range_query!(
                    &mut conn,
                    tx_kinds,
                    tx_sequence_number,
                    prune_min,
                    prune_max
                )
            }
            PrunableTable::Checkpoints => {
                execute_delete_range_query!(
                    &mut conn,
                    checkpoints,
                    sequence_number,
                    prune_min,
                    prune_max
                )
            }
        } {
            error!("Failed to prune table {}: {}", self.table, err);
        };

        Ok(())
    }
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
        let retention_policies = retention_config.retention_policies();

        Ok(Self {
            store,
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

        let mut table_tasks = vec![];

        for table in PrunableTable::iter() {
            let store_clone = self.store.clone();
            let partition_manager_clone = self.partition_manager.clone();
            let cancel_clone = cancel.clone();
            let mut table_pruner =
                TablePruner::new(table, store_clone, partition_manager_clone, cancel_clone);

            table_tasks.push(spawn_monitored_task!(table_pruner.run()));

            let store_clone = self.store.clone();
            let cancel_clone = cancel.clone();

            table_tasks.push(spawn_monitored_task!(update_pruner_watermark_task(
                store_clone,
                table,
                cancel_clone
            )));
        }

        cancel.cancelled().await;

        join_all(table_tasks).await;

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
                if let Err(err) = update_watermarks_lower_bounds(&store, &retention_policies, &cancel).await {
                    error!("Failed to update watermarks lower bounds: {}", err);
                }
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
            info!("Reader watermark lower bound update task cancelled.");
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

/// Task to periodically update `pruner_hi` to the local `reader_lo` if it sees a newer
/// value for `reader_lo`.
async fn update_pruner_watermark_task(
    store: PgIndexerStore,
    table: PrunableTable,
    cancel: CancellationToken,
) -> IndexerResult<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let (watermark, _) = store.get_watermark(table).await?;
    let mut pruner_hi = watermark.pruner_upper_bound().unwrap();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Pruner watermark lower bound update task cancelled.");
                return Ok(());
            }
            _ = interval.tick() => {
                let (watermark, latest_db_timestamp) = store.get_watermark(table).await?;
                let reader_lo_timestamp = watermark.timestamp_ms;
                let new_pruner_hi = watermark.pruner_upper_bound().unwrap();

                // Only update if the new prune_max is greater than what we have locally
                if new_pruner_hi > pruner_hi {
                    // TODO: (wlmyng) use config value
                    let delay_duration = MAX_DELAY_MS.saturating_sub((latest_db_timestamp - reader_lo_timestamp) as u64);

                    if delay_duration > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_duration)).await;
                    }

                    if let Err(err) = store.update_pruner_watermark(table, new_pruner_hi).await {
                        error!("Failed to update pruner watermark: {}", err);
                    }
                    pruner_hi = new_pruner_hi;
                }
            }
        }
    }
}
