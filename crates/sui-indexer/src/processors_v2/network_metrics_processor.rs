// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::{info, warn};

use crate::models_v2::network_metrics::StoredNetworkMetrics;
use crate::models_v2::tx_count_metrics::{StoredTxCountMetrics, TxCountMetricsDelta};
use crate::schema::checkpoint_metrics::peak_tps_30d;
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

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
        loop {
            let latest_tx_count_metrics = self.store.get_latest_tx_count_metrics().await?;
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < latest_tx_count_metrics.checkpoint_sequence_number + 100
            {
                std::sleep::sleep(std::time::Duration::from_secs(1));
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }
            // +1 here b/c get_transactions_in_checkpoint_range is left-inclusive, right-exclusive,
            // but we want left-exclusive, right-inclusive, as latest_tx_count_metrics has been processed.
            let tx_batch = self
                .store
                .get_transactions_in_checkpoint_range(
                    latest_tx_count_metrics.checkpoint_sequence_number + 1,
                    latest_stored_checkpoint.sequence_number + 1,
                )
                .await?;
            let tx_count_metrics_delta = TxCountMetricsDelta::get_tx_count_metrics_delta(
                &tx_batch,
                &latest_stored_checkpoint,
            );
            let tx_count_metrics = StoredTxCountMetrics::combine_tx_count_metrics_delta(
                &latest_tx_count_metrics,
                &tx_count_metrics_delta,
            );
            self.store
                .persist_tx_count_metrics(tx_count_metrics)
                .await?;

            let real_time_tps = (tx_count_metrics_delta.total_successful_transactions
                + tx_count_metrics_delta.total_transaction_blocks
                - tx_count_metrics_delta.total_successful_transaction_blocks)
                * 1000.0f64
                / (tx_count_metrics_delta.timestamp_ms - latest_tx_count_metrics.timestamp_ms)
                    as f64;
            let prev_peak_30d_tps = self
                .store
                .get_peak_30d_tps(latest_stored_checkpoint.epoch)
                .await?;
            let peak_tps_30d = max(real_time_tps, prev_peak_30d_tps);
            let estimated_object_count = self.store.get_estimated_count("objects").await?;
            let estimated_packages_count = self.store.get_estimated_count("packages").await?;
            let estimated_addresses_count = self.store.get_estimated_count("addresses").await?;

            let network_metrics = StoredNetworkMetrics {
                checkpoint: latest_stored_checkpoint.sequence_number,
                epoch: latest_stored_checkpoint.epoch,
                timestamp_ms: tx_count_metrics.timestamp_ms,
                real_time_tps,
                peak_tps_30d,
                total_objects: estimated_object_count as i64,
                total_addresses: estimated_addresses_count as i64,
                total_packages: estimated_packages_count as i64,
            };
            self.store.persist_network_metrics(network_metrics).await?;
            latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            info!(
                "Processed checkpoint for network_metrics: {}",
                latest_stored_checkpoint.sequence_number
            );
        }
    }
}
