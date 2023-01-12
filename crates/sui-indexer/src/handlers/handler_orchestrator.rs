// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_indexer::PgConnectionPool;
use sui_sdk::SuiClient;

use backoff::future::retry;
use backoff::ExponentialBackoff;
use futures::future::try_join_all;
use prometheus::Registry;
use tracing::{error, info, warn};

use crate::handlers::event_handler::EventHandler;
use crate::handlers::transaction_handler::TransactionHandler;

#[derive(Clone)]
pub struct HandlerOrchestrator {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
    prometheus_registry: Registry,
}

impl HandlerOrchestrator {
    pub fn new(
        rpc_client: SuiClient,
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: Registry,
    ) -> Self {
        Self {
            rpc_client,
            pg_connection_pool,
            prometheus_registry,
        }
    }

    pub async fn run_forever(&self) {
        info!("Handler orchestrator started...");
        let event_handler = EventHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );
        let txn_handler = TransactionHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );

        let txn_handle = tokio::task::spawn(async move {
            let txn_res = retry(ExponentialBackoff::default(), || async {
                let txn_handler_exec_res = txn_handler.start().await;
                if let Err(e) = txn_handler_exec_res.clone() {
                    txn_handler
                        .transaction_handler_metrics
                        .total_transaction_handler_error
                        .inc();
                    warn!(
                        "Indexer transaction handler failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(txn_handler_exec_res?)
            })
            .await;
            if let Err(e) = txn_res {
                error!(
                    "Indexer transaction handler failed after retrials with error: {:?}!",
                    e
                );
            }
        });
        let event_handle = tokio::task::spawn(async move {
            let event_res = retry(ExponentialBackoff::default(), || async {
                let event_handler_exec_res = event_handler.start().await;
                if let Err(e) = event_handler_exec_res.clone() {
                    event_handler
                        .event_handler_metrics
                        .total_event_handler_error
                        .inc();
                    warn!(
                        "Indexer event handler failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(event_handler_exec_res?)
            })
            .await;
            if let Err(e) = event_res {
                error!(
                    "Indexer event handler failed after retrials with error: {:?}",
                    e
                );
            }
        });
        try_join_all(vec![txn_handle, event_handle])
            .await
            .expect("Handler orchestrator shoult not run into errors.");
    }
}
