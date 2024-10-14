// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use anyhow::Result;
use axum::{extract::Extension, routing::get, Router};
use mysten_metrics::RegistryService;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, Histogram, IntCounter, IntCounterVec, Registry,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Histogram buckets for the distribution of checkpoint fetching latencies.
const INGESTION_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
];

/// Service to expose prometheus metrics from the indexer.
pub struct MetricsService {
    addr: SocketAddr,
    service: RegistryService,
    cancel: CancellationToken,
}

#[derive(Clone)]
pub struct IndexerMetrics {
    // Statistics related to fetching data from the remote store.
    pub total_ingested_checkpoints: IntCounter,
    pub total_ingested_transactions: IntCounter,
    pub total_ingested_events: IntCounter,
    pub total_ingested_inputs: IntCounter,
    pub total_ingested_outputs: IntCounter,
    pub total_ingested_bytes: IntCounter,
    pub total_ingested_transient_retries: IntCounterVec,
    pub total_ingested_not_found_retries: IntCounter,

    // Distribution of times taken to fetch data from the remote store, including time taken on
    // retries.
    pub ingested_checkpoint_latency: Histogram,
}

impl MetricsService {
    /// Create a new metrics service, exposing Mysten-wide metrics, and Indexer-specific metrics.
    /// Returns the Indexer-specific metrics and the service itself (which must be run with
    /// [Self::run]).
    pub fn new(
        addr: SocketAddr,
        cancel: CancellationToken,
    ) -> Result<(IndexerMetrics, MetricsService)> {
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None)?;
        let metrics = IndexerMetrics::new(&registry);
        mysten_metrics::init_metrics(&registry);

        let service = Self {
            addr,
            service: RegistryService::new(registry),
            cancel,
        };

        Ok((metrics, service))
    }

    /// Start the service. The service will run until the cancellation token is triggered.
    pub async fn run(self) -> Result<JoinHandle<()>> {
        let listener = TcpListener::bind(&self.addr).await?;
        let app = Router::new()
            .route("/metrics", get(mysten_metrics::metrics))
            .layer(Extension(self.service));

        Ok(tokio::spawn(async move {
            info!("Starting metrics service on {}", self.addr);
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    self.cancel.cancelled().await;
                    info!("Shutdown received, stopping metrics service");
                })
                .await
                .unwrap();
        }))
    }
}

impl IndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_ingested_checkpoints: register_int_counter_with_registry!(
                "indexer_total_ingested_checkpoints",
                "Total number of checkpoints fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_transactions: register_int_counter_with_registry!(
                "indexer_total_ingested_transactions",
                "Total number of transactions fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_events: register_int_counter_with_registry!(
                "indexer_total_ingested_events",
                "Total number of events fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_inputs: register_int_counter_with_registry!(
                "indexer_total_ingested_inputs",
                "Total number of input objects fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_outputs: register_int_counter_with_registry!(
                "indexer_total_ingested_outputs",
                "Total number of output objects fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_bytes: register_int_counter_with_registry!(
                "indexer_total_ingested_bytes",
                "Total number of bytes fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_transient_retries: register_int_counter_vec_with_registry!(
                "indexer_total_ingested_retries",
                "Total number of retries due to transient errors while fetching data from the \
                 remote store",
                &["reason"],
                registry,
            )
            .unwrap(),
            total_ingested_not_found_retries: register_int_counter_with_registry!(
                "indexer_total_ingested_not_found_retries",
                "Total number of retries due to the not found errors while fetching data from the \
                 remote store",
                registry,
            )
            .unwrap(),
            ingested_checkpoint_latency: register_histogram_with_registry!(
                "indexer_ingested_checkpoint_latency",
                "Time taken to fetch a checkpoint from the remote store, including retries",
                INGESTION_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }

    /// Register that we're retrying a checkpoint fetch due to a transient error.
    pub(crate) fn inc_retry(&self, checkpoint: u64, reason: &str) {
        warn!(checkpoint, reason, "Transient error, retrying...");
        self.total_ingested_transient_retries
            .with_label_values(&[reason])
            .inc();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use prometheus::Registry;

    use super::IndexerMetrics;

    /// Construct metrics for test purposes.
    pub fn test_metrics() -> IndexerMetrics {
        IndexerMetrics::new(&Registry::new())
    }
}
