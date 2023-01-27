// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use futures::future::try_join_all;
use prometheus::Registry;
use std::sync::Arc;
use sui_indexer::PgConnectionPool;
use sui_sdk::SuiClient;
use tracing::{error, info, warn};

use crate::processors::address_processor::AddressProcessor;
use crate::processors::object_processor::ObjectProcessor;
use crate::processors::package_processor::PackageProcessor;
use crate::processors::transaction_stats_processor::TransactionStatsProcessor;

pub struct ProcessorOrchestrator {
    rpc_client: SuiClient,
    conn_pool: Arc<PgConnectionPool>,
    prometheus_registry: Registry,
}

impl ProcessorOrchestrator {
    pub fn new(
        rpc_client: SuiClient,
        conn_pool: Arc<PgConnectionPool>,
        prometheus_registry: Registry,
    ) -> Self {
        Self {
            rpc_client,
            conn_pool,
            prometheus_registry,
        }
    }

    pub async fn run_forever(&mut self) {
        info!("Processor orchestrator started...");
        let address_processor =
            AddressProcessor::new(self.conn_pool.clone(), &self.prometheus_registry);
        let object_processor =
            ObjectProcessor::new(self.conn_pool.clone(), &self.prometheus_registry);
        let package_processor = PackageProcessor::new(
            self.rpc_client.clone(),
            self.conn_pool.clone(),
            &self.prometheus_registry,
        );
        let transaction_stats_processor =
            TransactionStatsProcessor::new(self.conn_pool.clone(), &self.prometheus_registry);

        let addr_handle = tokio::task::spawn(async move {
            let addr_result = retry(ExponentialBackoff::default(), || async {
                let addr_processor_exec_res = address_processor.start().await;
                if let Err(e) = addr_processor_exec_res.clone() {
                    address_processor
                        .address_processor_metrics
                        .total_address_processor_error
                        .inc();
                    warn!(
                        "Indexer address processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(addr_processor_exec_res?)
            })
            .await;
            if let Err(e) = addr_result {
                error!(
                    "Indexer address processor failed after retrials with error {:?}",
                    e
                );
            }
        });
        let obj_handle = tokio::task::spawn(async move {
            let obj_result = retry(ExponentialBackoff::default(), || async {
                let obj_processor_exec_res = object_processor.start().await;
                if let Err(e) = obj_processor_exec_res.clone() {
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
        let pkg_handle = tokio::task::spawn(async move {
            let pkg_result = retry(ExponentialBackoff::default(), || async {
                let pkg_processor_exec_res = package_processor.start().await;
                if let Err(e) = pkg_processor_exec_res.clone() {
                    package_processor
                        .package_processor_metrics
                        .total_package_processor_error
                        .inc();
                    warn!(
                        "Indexer package processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(pkg_processor_exec_res?)
            })
            .await;
            if let Err(e) = pkg_result {
                error!(
                    "Indexer package processor failed after retrials with error {:?}",
                    e
                );
            }
        });
        let txn_stats_handle = tokio::task::spawn(async move {
            let txn_stats_result = retry(ExponentialBackoff::default(), || async {
                let txn_stats_processor_exec_res = transaction_stats_processor.start().await;
                if let Err(e) = txn_stats_processor_exec_res.clone() {
                    transaction_stats_processor
                        .transaction_stats_processor_metrics
                        .total_transaction_stats_error
                        .inc();
                    warn!(
                        "Indexer transaction stats processor failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(txn_stats_processor_exec_res?)
            })
            .await;
            if let Err(e) = txn_stats_result {
                error!(
                    "Indexer transaction stats processor failed after retrials with error {:?}",
                    e
                );
            }
        });
        try_join_all(vec![addr_handle, pkg_handle, obj_handle, txn_stats_handle])
            .await
            .expect("Processor orchestrator should not run into errors.");
    }
}
