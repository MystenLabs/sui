// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_indexer::PgConnectionPool;
use sui_sdk::SuiClient;

use backoff::future::retry;
use backoff::ExponentialBackoffBuilder;
use futures::future::try_join_all;
use prometheus::Registry;
use tracing::{error, info, warn};

use crate::handlers::checkpoint_handler::CheckpointHandler;
use crate::handlers::event_handler::EventHandler;
use crate::handlers::transaction_handler::TransactionHandler;

use crate::handlers::move_event_handler::MoveEventHandler;
use crate::handlers::object_event_handler::ObjectEventHandler;
use crate::handlers::publish_event_handler::PublishEventHandler;
const BACKOFF_MAX_INTERVAL_IN_SECS: u64 = 600;

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
        let checkpoint_handler = CheckpointHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );
        let event_handler = EventHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );
        let obj_event_handler = ObjectEventHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );
        let publish_event_handler = PublishEventHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );
        let txn_handler = TransactionHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );
        let move_handler = MoveEventHandler::new(
            self.rpc_client.clone(),
            self.pg_connection_pool.clone(),
            &self.prometheus_registry,
        );

        let checkpoint_handle = tokio::task::spawn(async move {
            let backoff_config = ExponentialBackoffBuilder::new()
                .with_max_interval(std::time::Duration::from_secs(BACKOFF_MAX_INTERVAL_IN_SECS))
                .build();
            let checkpoint_res = retry(backoff_config, || async {
                let checkpoint_handler_exec_res = checkpoint_handler.start().await;
                if let Err(e) = checkpoint_handler_exec_res.clone() {
                    checkpoint_handler
                        .checkpoint_handler_metrics
                        .total_checkpoint_handler_error
                        .inc();
                    warn!(
                        "Indexer checkpoint handler failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(checkpoint_handler_exec_res?)
            })
            .await;
            if let Err(e) = checkpoint_res {
                error!(
                    "Indexer checkpoint handler failed after retrials with error: {:?}!",
                    e
                );
            }
        });
        let txn_handle = tokio::task::spawn(async move {
            let backoff_config = ExponentialBackoffBuilder::new()
                .with_max_interval(std::time::Duration::from_secs(BACKOFF_MAX_INTERVAL_IN_SECS))
                .build();
            let txn_res = retry(backoff_config, || async {
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
            let backoff_config = ExponentialBackoffBuilder::new()
                .with_max_interval(std::time::Duration::from_secs(BACKOFF_MAX_INTERVAL_IN_SECS))
                .build();
            let event_res = retry(backoff_config, || async {
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
        let object_event_handle = tokio::task::spawn(async move {
            let backoff_config = ExponentialBackoffBuilder::new()
                .with_max_interval(std::time::Duration::from_secs(BACKOFF_MAX_INTERVAL_IN_SECS))
                .build();
            let object_event_res = retry(backoff_config, || async {
                let object_event_handler_exec_res = obj_event_handler.start().await;
                if let Err(e) = object_event_handler_exec_res.clone() {
                    obj_event_handler
                        .event_handler_metrics
                        .total_object_event_handler_error
                        .inc();
                    warn!(
                        "Indexer object event handler failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(object_event_handler_exec_res?)
            })
            .await;
            if let Err(e) = object_event_res {
                error!(
                    "Indexer object event handler failed after retrials with error: {:?}",
                    e
                );
            }
        });
        let publish_event_handle = tokio::task::spawn(async move {
            let backoff_config = ExponentialBackoffBuilder::new()
                .with_max_interval(std::time::Duration::from_secs(BACKOFF_MAX_INTERVAL_IN_SECS))
                .build();
            let publish_event_res = retry(backoff_config, || async {
                let publish_event_handler_exec_res = publish_event_handler.start().await;
                if let Err(e) = publish_event_handler_exec_res.clone() {
                    publish_event_handler
                        .event_handler_metrics
                        .total_publish_event_handler_error
                        .inc();
                    warn!(
                        "Indexer publish event handler failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(publish_event_handler_exec_res?)
            })
            .await;
            if let Err(e) = publish_event_res {
                error!(
                    "Indexer publish event handler failed after retrials with error: {:?}",
                    e
                );
            }
        });
        let move_event_handle = tokio::task::spawn(async move {
            let backoff_config = ExponentialBackoffBuilder::new()
                .with_max_interval(std::time::Duration::from_secs(BACKOFF_MAX_INTERVAL_IN_SECS))
                .build();
            let move_event_res = retry(backoff_config, || async {
                let move_event_handler_exec_res = move_handler.start().await;
                if let Err(e) = move_event_handler_exec_res.clone() {
                    move_handler
                        .event_handler_metrics
                        .total_move_event_handler_error
                        .inc();
                    warn!(
                        "Indexer move event handler failed with error: {:?}, retrying...",
                        e
                    );
                }
                Ok(move_event_handler_exec_res?)
            })
            .await;
            if let Err(e) = move_event_res {
                error!(
                    "Indexer move event handler failed after retrials with error: {:?}",
                    e
                );
            }
        });
        try_join_all(vec![
            txn_handle,
            event_handle,
            checkpoint_handle,
            object_event_handle,
            publish_event_handle,
            move_event_handle,
        ])
        .await
        .expect("Handler orchestrator shoult not run into errors.");
    }
}
