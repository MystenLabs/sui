// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::{metrics::IndexerMetrics, store::IndexerStore, types::IndexerResult};

pub struct Pruner<S> {
    pub store: S,
    pub epochs_to_keep: u64,
    pub metrics: IndexerMetrics,
}

impl<S> Pruner<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S, epochs_to_keep: u64, metrics: IndexerMetrics) -> Self {
        Self {
            store,
            epochs_to_keep,
            metrics,
        }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        loop {
            let (mut min_epoch, mut max_epoch) = self.store.get_available_epoch_range().await?;
            while min_epoch + self.epochs_to_keep > max_epoch {
                tokio::time::sleep(Duration::from_secs(5)).await;
                (min_epoch, max_epoch) = self.store.get_available_epoch_range().await?;
            }

            for epoch in min_epoch..=max_epoch - self.epochs_to_keep {
                tracing::info!("Pruning epoch {}", epoch);
                self.store.prune_epoch(epoch).await.unwrap_or_else(|e| {
                    tracing::error!("Failed to prune epoch {}: {}", epoch, e);
                });
                self.metrics.last_pruned_epoch.set(epoch as i64);
                tracing::info!("Pruned epoch {}", epoch);
            }
        }
    }
}
