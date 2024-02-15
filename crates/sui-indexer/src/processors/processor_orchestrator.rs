// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::try_join_all;
use tracing::{error, info};

use crate::metrics::IndexerMetrics;
use crate::store::IndexerAnalyticalStore;

use super::address_metrics_processor::AddressMetricsProcessor;
use super::move_call_metrics_processor::MoveCallMetricsProcessor;
use super::network_metrics_processor::NetworkMetricsProcessor;

pub struct ProcessorOrchestrator<S> {
    store: S,
    metrics: IndexerMetrics,
}

impl<S> ProcessorOrchestrator<S>
where
    S: IndexerAnalyticalStore + Clone + Send + Sync + 'static,
{
    pub fn new(store: S, metrics: IndexerMetrics) -> Self {
        Self { store, metrics }
    }

    pub async fn run_forever(&mut self) {
        info!("Processor orchestrator started...");
        let network_metrics_processor =
            NetworkMetricsProcessor::new(self.store.clone(), self.metrics.clone());
        let network_metrics_handle = tokio::task::spawn(async move {
            loop {
                let network_metrics_res = network_metrics_processor.start().await;
                if let Err(e) = network_metrics_res {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    error!(
                        "Indexer network metrics processor failed with error {:?}, retrying in 5s...",
                        e
                    );
                }
            }
        });

        let addr_metrics_processor =
            AddressMetricsProcessor::new(self.store.clone(), self.metrics.clone());
        let addr_metrics_handle = tokio::task::spawn(async move {
            loop {
                let addr_metrics_res = addr_metrics_processor.start().await;
                if let Err(e) = addr_metrics_res {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    error!(
                        "Indexer address metrics processor failed with error {:?}, retrying in 5s...",
                        e
                    );
                }
            }
        });

        let move_call_metrics_processor =
            MoveCallMetricsProcessor::new(self.store.clone(), self.metrics.clone());
        let move_call_metrics_handle = tokio::task::spawn(async move {
            loop {
                let move_call_metrics_res = move_call_metrics_processor.start().await;
                if let Err(e) = move_call_metrics_res {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    error!(
                        "Indexer move call metrics processor failed with error {:?}, retrying in 5s...",
                        e
                    );
                }
            }
        });

        try_join_all(vec![
            network_metrics_handle,
            addr_metrics_handle,
            move_call_metrics_handle,
        ])
        .await
        .expect("Processor orchestrator should not run into errors.");
    }
}
