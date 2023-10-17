// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::try_join_all;
use tokio::time::sleep;
use tracing::{error, info};

use crate::metrics::IndexerMetrics;
use crate::processors::address_processor::AddressProcessor;
use crate::processors::checkpoint_metrics_processor::CheckpointMetricsProcessor;
use crate::store::IndexerStore;

pub struct ProcessorOrchestrator<S> {
    store: S,
    metrics: IndexerMetrics,
}

impl<S> ProcessorOrchestrator<S>
where
    S: IndexerStore + Send + Sync + 'static + Clone,
{
    pub fn new(store: S, metrics: IndexerMetrics) -> Self {
        Self { store, metrics }
    }

    pub async fn run_forever(&mut self) {
        info!("Processor orchestrator started...");
        let address_stats_processor = AddressProcessor::new(self.store.clone());
        let cp_metrics_processor = CheckpointMetricsProcessor::new(self.store.clone());

        let metrics_clone = self.metrics.clone();
        let addr_handle = tokio::task::spawn(async move {
            loop {
                let addr_stats_exec_res = address_stats_processor.start().await;
                if let Err(e) = &addr_stats_exec_res {
                    error!(
                        "Indexer address stats processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                metrics_clone.address_processor_failure.inc();
                sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });

        let metrics_clone = self.metrics.clone();
        let cp_metrics_handle = tokio::task::spawn(async move {
            loop {
                let cp_metrics_exec_res = cp_metrics_processor.start().await;
                if let Err(e) = &cp_metrics_exec_res {
                    error!(
                        "Indexer checkpoint metrics processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                metrics_clone.checkpoint_metrics_processor_failure.inc();
                sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
        try_join_all(vec![addr_handle, cp_metrics_handle])
            .await
            .expect("Processor orchestrator should not run into errors.");
    }
}
