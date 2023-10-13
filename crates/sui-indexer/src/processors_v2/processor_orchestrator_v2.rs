// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::try_join_all;
use tracing::{error, info};

use crate::store::IndexerAnalyticalStore;

use super::address_metrics_processor::AddressMetricsProcessor;
use super::move_call_metrics_processor::MoveCallMetricsProcessor;
use super::network_metrics_processor::NetworkMetricsProcessor;

pub struct ProcessorOrchestratorV2<S> {
    store: S,
}

impl<S> ProcessorOrchestratorV2<S>
where
    S: IndexerAnalyticalStore + Send + Sync + 'static + Clone,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn run_forever(&mut self) {
        info!("Processor orchestrator started...");
        // TODO(gegaowp): add metrics for each processor to monitor health and progress
        loop {
            let network_metrics_processor = NetworkMetricsProcessor::new(self.store.clone());
            let network_metrics_handle = tokio::task::spawn(async move {
                let network_metrics_res = network_metrics_processor.start().await;
                if let Err(e) = network_metrics_res {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    error!(
                        "Indexer network metrics processor failed with error {:?}, retrying in 5s...",
                        e
                    );
                }
            });

            let addr_metrics_processor = AddressMetricsProcessor::new(self.store.clone());
            let addr_metrics_handle = tokio::task::spawn(async move {
                let addr_metrics_res = addr_metrics_processor.start().await;
                if let Err(e) = addr_metrics_res {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    error!(
                        "Indexer address metrics processor failed with error {:?}, retrying in 5s...",
                        e
                    );
                }
            });

            let move_call_metrics_processor = MoveCallMetricsProcessor::new(self.store.clone());
            let move_call_metrics_handle = tokio::task::spawn(async move {
                let move_call_metrics_res = move_call_metrics_processor.start().await;
                if let Err(e) = move_call_metrics_res {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    error!(
                        "Indexer move call metrics processor failed with error {:?}, retrying in 5s...",
                        e
                    );
                }
            });

            let processor_orchestrator_res = try_join_all(vec![
                network_metrics_handle,
                addr_metrics_handle,
                move_call_metrics_handle,
            ])
            .await;
            if let Err(e) = processor_orchestrator_res {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                error!(
                    "Indexer processor orchestrator failed with error {:?}, retrying in 10s...",
                    e
                );
            }
        }
    }
}
