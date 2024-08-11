// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use diesel::r2d2::R2D2Connection;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::errors::IndexerError;
use crate::store::pg_partition_manager::PgPartitionManager;
use crate::{metrics::IndexerMetrics, store::IndexerStore, types::IndexerResult};

use super::checkpoint_handler::CheckpointHandler;

pub struct Pruner<S, T: R2D2Connection + 'static> {
    pub store: S,
    pub partition_manager: PgPartitionManager<T>,
    pub epochs_to_keep: u64,
    pub metrics: IndexerMetrics,
}

impl<S, T> Pruner<S, T>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
    T: R2D2Connection + 'static,
{
    pub fn new(
        store: S,
        epochs_to_keep: u64,
        metrics: IndexerMetrics,
    ) -> Result<Self, IndexerError> {
        let blocking_cp = CheckpointHandler::<S, T>::pg_blocking_cp(store.clone()).unwrap();
        let partition_manager = PgPartitionManager::new(blocking_cp.clone())?;
        Ok(Self {
            store,
            partition_manager,
            epochs_to_keep,
            metrics,
        })
    }

    pub async fn start(&self, cancel: CancellationToken) -> IndexerResult<()> {
        loop {
            if cancel.is_cancelled() {
                info!("Pruner task cancelled.");
                return Ok(());
            }

            let (mut min_epoch, mut max_epoch) = self.store.get_available_epoch_range().await?;
            while min_epoch + self.epochs_to_keep > max_epoch {
                if cancel.is_cancelled() {
                    info!("Pruner task cancelled.");
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                (min_epoch, max_epoch) = self.store.get_available_epoch_range().await?;
            }

            let table_partitions = self.partition_manager.get_table_partitions()?;
            let table_names = table_partitions.keys().cloned().collect::<Vec<_>>();
            for (table_name, (min_partition, max_partition)) in table_partitions {
                if max_epoch != max_partition {
                    error!(
                        "Epochs are out of sync for table {}: max_epoch={}, max_partition={}",
                        table_name, max_epoch, max_partition
                    );
                }
                // drop partitions if pruning is enabled afterwards, where all epochs before min_epoch
                // would have been pruned already if the pruner was running.
                for epoch in min_partition..min_epoch {
                    self.partition_manager
                        .drop_table_partition(table_name.clone(), epoch)?;
                    info!(
                        "Batch dropped table partition {} epoch {}",
                        table_name, epoch
                    );
                }
            }

            for epoch in min_epoch..max_epoch.saturating_sub(self.epochs_to_keep - 1) {
                if cancel.is_cancelled() {
                    info!("Pruner task cancelled.");
                    return Ok(());
                }
                info!("Pruning epoch {}", epoch);
                for table_name in table_names.clone() {
                    self.partition_manager
                        .drop_table_partition(table_name.clone(), epoch)?;
                    info!("Dropped table partition {} epoch {}", table_name, epoch);
                }
                self.store.prune_epoch(epoch).await.unwrap_or_else(|e| {
                    error!("Failed to prune epoch {}: {}", epoch, e);
                });
                self.metrics.last_pruned_epoch.set(epoch as i64);
                info!("Pruned epoch {}", epoch);
            }
        }
    }
}
