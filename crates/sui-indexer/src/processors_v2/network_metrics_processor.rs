// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use tracing::info;

use crate::errors::IndexerError;
use crate::models_v2::network_metrics::StoredNetworkMetrics;
use crate::models_v2::tx_count_metrics::{StoredTxCountMetrics, TxCountMetricsDelta};
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

const NETWORK_METRICS_PROCESSOR_BATCH_SIZE: i64 = 100;

pub struct NetworkMetricsProcessor<S> {
    pub store: S,
}

impl<S> NetworkMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> NetworkMetricsProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer network metrics async processor started...");
        let mut latest_tx_count_metrics = self
            .store
            .get_latest_tx_count_metrics()
            .await
            .unwrap_or_default();
        let mut last_end_cp_seq = latest_tx_count_metrics.checkpoint_sequence_number;
        loop {
            // NOTE: network metrics include address count, which is handled by populated by address metrics processor.
            let mut latest_address_metrics = self
                .store
                .get_latest_address_metrics()
                .await
                .unwrap_or_default();
            while latest_address_metrics.checkpoint
                < last_end_cp_seq + NETWORK_METRICS_PROCESSOR_BATCH_SIZE
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_address_metrics = self
                    .store
                    .get_latest_address_metrics()
                    .await
                    .unwrap_or_default();
            }

            let end_cp = self
                .store
                .get_checkpoints_in_range(
                    last_end_cp_seq + NETWORK_METRICS_PROCESSOR_BATCH_SIZE,
                    last_end_cp_seq + NETWORK_METRICS_PROCESSOR_BATCH_SIZE + 1,
                )
                .await?
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read checkpoint from PG for address metrics".to_string(),
                ))?
                .clone();

            // +1 here b/c get_tx_success_cmd_counts_in_checkpoint_range is left-inclusive, right-exclusive,
            // but we want left-exclusive, right-inclusive, as latest_tx_count_metrics has been processed.
            let tx_cmd_count_batch = self
                .store
                .get_tx_success_cmd_counts_in_checkpoint_range(
                    last_end_cp_seq + 1,
                    end_cp.sequence_number + 1,
                )
                .await?;
            let tx_count_metrics_delta =
                TxCountMetricsDelta::get_tx_count_metrics_delta(&tx_cmd_count_batch, &end_cp);
            let tx_count_metrics = StoredTxCountMetrics::combine_tx_count_metrics_delta(
                &latest_tx_count_metrics,
                &tx_count_metrics_delta,
            );
            self.store
                .persist_tx_count_metrics(tx_count_metrics.clone())
                .await?;

            let real_time_tps = (tx_count_metrics_delta.total_successful_transactions
                + tx_count_metrics_delta.total_transaction_blocks
                - tx_count_metrics_delta.total_successful_transaction_blocks)
                as f64
                * 1000.0f64
                / (tx_count_metrics_delta.timestamp_ms - latest_tx_count_metrics.timestamp_ms)
                    as f64;
            let prev_peak_30d_tps = self
                .store
                .get_peak_network_peak_tps(end_cp.epoch, 30)
                .await?;
            let peak_tps_30d = f64::max(real_time_tps, prev_peak_30d_tps);
            let estimated_object_count = self.store.get_estimated_count("objects").await?;
            let estimated_packages_count = self.store.get_estimated_count("packages").await?;
            let estimated_addresses_count = self.store.get_estimated_count("addresses").await?;

            let network_metrics = StoredNetworkMetrics {
                checkpoint: end_cp.sequence_number,
                epoch: end_cp.epoch,
                timestamp_ms: end_cp.timestamp_ms,
                real_time_tps,
                peak_tps_30d,
                total_objects: estimated_object_count,
                total_addresses: estimated_addresses_count,
                total_packages: estimated_packages_count,
            };
            self.store.persist_network_metrics(network_metrics).await?;
            last_end_cp_seq = end_cp.sequence_number;
            latest_tx_count_metrics = tx_count_metrics;
            info!(
                "Processed checkpoint for network_metrics: {}",
                end_cp.sequence_number
            );
        }
    }
}
