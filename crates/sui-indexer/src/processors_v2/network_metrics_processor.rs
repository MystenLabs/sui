// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use tracing::info;

use crate::errors::IndexerError;
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

const NETWORK_METRICS_PROCESSOR_BATCH_SIZE: i64 = 10;

pub struct NetworkMetricsProcessor<S> {
    pub store: S,
    pub network_processor_metrics_batch_size: i64,
}

impl<S> NetworkMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> NetworkMetricsProcessor<S> {
        let network_processor_metrics_batch_size =
            std::env::var("NETWORK_PROCESSOR_METRICS_BATCH_SIZE")
                .map(|s| {
                    s.parse::<i64>()
                        .unwrap_or(NETWORK_METRICS_PROCESSOR_BATCH_SIZE)
                })
                .unwrap_or(NETWORK_METRICS_PROCESSOR_BATCH_SIZE);
        Self {
            store,
            network_processor_metrics_batch_size,
        }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer network metrics async processor started...");
        let latest_tx_count_metrics = self
            .store
            .get_latest_tx_count_metrics()
            .await
            .unwrap_or_default();
        let latest_epoch_peak_tps = self
            .store
            .get_latest_epoch_peak_tps()
            .await
            .unwrap_or_default();
        let mut last_processed_cp_seq = latest_tx_count_metrics.checkpoint_sequence_number;
        let mut last_processed_peak_tps_epoch = latest_epoch_peak_tps.epoch;
        loop {
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < last_processed_cp_seq + self.network_processor_metrics_batch_size
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }
            self.store
                .persist_tx_count_metrics(
                    last_processed_cp_seq + 1,
                    last_processed_cp_seq + self.network_processor_metrics_batch_size,
                )
                .await?;
            last_processed_cp_seq += self.network_processor_metrics_batch_size;
            info!(
                "Persisted tx count metrics for checkpoint sequence number {}",
                last_processed_cp_seq
            );

            let end_cp = self
                .store
                .get_checkpoints_in_range(last_processed_cp_seq, last_processed_cp_seq + 1)
                .await?
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read checkpoint from PG for epoch peak TPS".to_string(),
                ))?
                .clone();
            for epoch in last_processed_peak_tps_epoch + 1..=end_cp.epoch {
                self.store.persist_epoch_peak_tps(epoch).await?;
                last_processed_peak_tps_epoch = epoch;
                info!("Persisted epoch peak TPS for epoch {}", epoch);
            }
        }
    }
}
