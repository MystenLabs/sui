// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tap::tap::TapFallible;
use tracing::{error, info};

use crate::errors::IndexerError;
use crate::metrics::IndexerMetrics;
use crate::store::IndexerAnalyticalStore;
use crate::types::IndexerResult;

const NETWORK_METRICS_PROCESSOR_BATCH_SIZE: usize = 10;
const PARALLELISM: usize = 1;

pub struct NetworkMetricsProcessor<S> {
    pub store: S,
    metrics: IndexerMetrics,
    pub network_processor_metrics_batch_size: usize,
    pub network_processor_metrics_parallelism: usize,
}

impl<S> NetworkMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S, metrics: IndexerMetrics) -> NetworkMetricsProcessor<S> {
        let network_processor_metrics_batch_size =
            std::env::var("NETWORK_PROCESSOR_METRICS_BATCH_SIZE")
                .map(|s| {
                    s.parse::<usize>()
                        .unwrap_or(NETWORK_METRICS_PROCESSOR_BATCH_SIZE)
                })
                .unwrap_or(NETWORK_METRICS_PROCESSOR_BATCH_SIZE);
        let network_processor_metrics_parallelism =
            std::env::var("NETWORK_PROCESSOR_METRICS_PARALLELISM")
                .map(|s| s.parse::<usize>().unwrap_or(PARALLELISM))
                .unwrap_or(PARALLELISM);
        Self {
            store,
            metrics,
            network_processor_metrics_batch_size,
            network_processor_metrics_parallelism,
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
        let mut last_processed_cp_seq = latest_tx_count_metrics
            .unwrap_or_default()
            .checkpoint_sequence_number;
        let mut last_processed_peak_tps_epoch = latest_epoch_peak_tps.unwrap_or_default().epoch;
        loop {
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while if let Some(cp) = latest_stored_checkpoint {
                cp.sequence_number
                    < last_processed_cp_seq + self.network_processor_metrics_batch_size as i64
            } else {
                true
            } {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }

            info!(
                "Persisting tx count metrics for checkpoint sequence number {}",
                last_processed_cp_seq
            );
            let batch_size = self.network_processor_metrics_batch_size;
            let step_size = batch_size / self.network_processor_metrics_parallelism;
            let mut persist_tasks = vec![];
            for chunk_start_cp in (last_processed_cp_seq + 1
                ..last_processed_cp_seq + batch_size as i64 + 1)
                .step_by(step_size)
            {
                let store = self.store.clone();
                persist_tasks.push(tokio::task::spawn_blocking(move || {
                    store
                        .persist_tx_count_metrics(chunk_start_cp, chunk_start_cp + step_size as i64)
                }));
            }
            futures::future::join_all(persist_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error joining network persist tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error persisting tx count metrics: {:?}", e);
                })?;
            last_processed_cp_seq += batch_size as i64;
            info!(
                "Persisted tx count metrics for checkpoint sequence number {}",
                last_processed_cp_seq
            );
            self.metrics
                .latest_network_metrics_cp_seq
                .set(last_processed_cp_seq);

            let end_cp = self
                .store
                .get_checkpoints_in_range(last_processed_cp_seq, last_processed_cp_seq + 1)
                .await?
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read checkpoint from PG for epoch peak TPS".to_string(),
                ))?
                .clone();
            for epoch in last_processed_peak_tps_epoch + 1..end_cp.epoch {
                self.store.persist_epoch_peak_tps(epoch).await?;
                last_processed_peak_tps_epoch = epoch;
                info!("Persisted epoch peak TPS for epoch {}", epoch);
            }
        }
    }
}
