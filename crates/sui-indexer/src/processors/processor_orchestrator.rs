// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use futures::future::try_join_all;
use prometheus::Registry;
use tracing::{error, info, warn};

use crate::processors::address_processor::AddressProcessor;
use crate::processors::checkpoint_metrics_processor::CheckpointMetricsProcessor;
use crate::processors::object_processor::ObjectProcessor;
use crate::store::IndexerStore;

pub struct ProcessorOrchestrator<S> {
    store: S,
    prometheus_registry: Registry,
}

impl<S> ProcessorOrchestrator<S>
where
    S: IndexerStore + Send + Sync + 'static + Clone,
{
    pub fn new(store: S, prometheus_registry: &Registry) -> Self {
        Self {
            store,
            prometheus_registry: prometheus_registry.clone(),
        }
    }

    pub async fn run_forever(&mut self) {
        info!("Processor orchestrator started...");
        let object_processor = ObjectProcessor::new(self.store.clone(), &self.prometheus_registry);
        let address_stats_processor = AddressProcessor::new(self.store.clone());
        let cp_metrics_processor = CheckpointMetricsProcessor::new(self.store.clone());

        // TODOggao: clean up object processor
        let obj_handle = tokio::task::spawn(async move {
            let obj_result = retry(ExponentialBackoff::default(), || async {
                let obj_processor_exec_res = object_processor.start().await;
                if let Err(e) = &obj_processor_exec_res {
                    object_processor
                        .object_processor_metrics
                        .total_object_processor_error
                        .inc();
                    warn!(
                        "Indexer object processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(obj_processor_exec_res?)
            })
            .await;
            if let Err(e) = obj_result {
                error!(
                    "Indexer object processor failed after retrials with error {:?}",
                    e
                );
            }
        });
        let addr_handle = tokio::task::spawn(async move {
            let addr_stats_result = retry(ExponentialBackoff::default(), || async {
                let addr_stats_exec_res = address_stats_processor.start().await;
                if let Err(e) = &addr_stats_exec_res {
                    warn!(
                        "Indexer address stats processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(addr_stats_exec_res?)
            })
            .await;
            if let Err(e) = addr_stats_result {
                error!(
                    "Indexer address stats processor failed after retries with error {:?}",
                    e
                );
            }
        });
        let cp_metrics_handle = tokio::task::spawn(async move {
            let cp_metrics_result = retry(ExponentialBackoff::default(), || async {
                let cp_metrics_exec_res = cp_metrics_processor.start().await;
                if let Err(e) = &cp_metrics_exec_res {
                    warn!(
                        "Indexer checkpoint metrics processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(cp_metrics_exec_res?)
            })
            .await;
            if let Err(e) = cp_metrics_result {
                error!(
                    "Indexer checkpoint metrics processor failed after retries with error {:?}",
                    e
                );
            }
        });
        try_join_all(vec![obj_handle, addr_handle, cp_metrics_handle])
            .await
            .expect("Processor orchestrator should not run into errors.");
    }
}
