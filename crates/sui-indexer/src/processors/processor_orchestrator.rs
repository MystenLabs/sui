// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::processors::object_processor::ObjectProcessor;
use backoff::future::retry;
use backoff::ExponentialBackoff;
use futures::future::try_join_all;
use prometheus::Registry;
use tracing::{error, info, warn};

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
        try_join_all(vec![obj_handle])
            .await
            .expect("Processor orchestrator should not run into errors.");
    }
}
