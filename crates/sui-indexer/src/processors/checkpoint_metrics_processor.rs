// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::{info, warn};

use crate::errors::IndexerError;
use crate::store::IndexerStore;

const CHECKPOINT_METRICS_BATCH_SIZE: usize = 100;
const DB_COMMIT_RETRY_INTERVAL_IN_MILLIS: u64 = 100;

pub struct CheckpointMetricsProcessor<S> {
    pub store: S,
}

impl<S> CheckpointMetricsProcessor<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> CheckpointMetricsProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer checkpoint metrics async processor started...");
        let mut last_cp_metrics = self
            .store
            .get_latest_checkpoint_metrics()
            .await
            .unwrap_or_default();
        let mut last_processed_cp = last_cp_metrics.checkpoint;
        // process another batch of events, 100 checkpoints at a time, otherwise sleep for 3 seconds
        loop {
            let latest_checkpoint = self
                .store
                .get_latest_tx_checkpoint_sequence_number()
                .await?;
            if latest_checkpoint >= last_processed_cp + CHECKPOINT_METRICS_BATCH_SIZE as i64 {
                let checkpoints = self
                    .store
                    .get_indexer_checkpoints(last_processed_cp, CHECKPOINT_METRICS_BATCH_SIZE)
                    .await?;

                let cp_metrics = self
                    .store
                    .calculate_checkpoint_metrics(
                        last_processed_cp + CHECKPOINT_METRICS_BATCH_SIZE as i64,
                        &last_cp_metrics,
                        &checkpoints,
                    )
                    .await?;

                let mut cp_metrics_commit_res =
                    self.store.persist_checkpoint_metrics(&cp_metrics).await;
                while let Err(e) = cp_metrics_commit_res {
                    warn!("Failed to commit checkpoint metrics to DB: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                    ))
                    .await;
                    cp_metrics_commit_res =
                        self.store.persist_checkpoint_metrics(&cp_metrics).await;
                }
                info!(
                    "Processed checkpoint metrics for checkpoint {}",
                    cp_metrics.checkpoint
                );
                last_processed_cp += CHECKPOINT_METRICS_BATCH_SIZE as i64;
                last_cp_metrics = cp_metrics;
            } else {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        }
    }
}
