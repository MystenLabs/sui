// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_indexer::PgConnectionPool;
use sui_sdk::SuiClient;

use futures::future::try_join_all;
use prometheus::Registry;
use tracing::{error, info, warn};

use crate::handlers::checkpoint_handler::CheckpointHandler;
use crate::handlers::event_handler::EventHandler;
use crate::handlers::transaction_handler::TransactionHandler;

use crate::handlers::move_event_handler::MoveEventHandler;
use crate::handlers::object_event_handler::ObjectEventHandler;
use crate::handlers::publish_event_handler::PublishEventHandler;

const HANDLER_RETRY_INTERVAL_IN_SECS: u64 = 10;

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
            let mut checkpoint_handler_exec_res = checkpoint_handler.start().await;
            while let Err(e) = checkpoint_handler_exec_res.clone() {
                warn!(
                    "Indexer checkpoint handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                checkpoint_handler_exec_res = checkpoint_handler.start().await;
            }
        });
        let txn_handle = tokio::task::spawn(async move {
            let mut txn_handler_exec_res = txn_handler.start().await;
            while let Err(e) = txn_handler_exec_res.clone() {
                warn!(
                    "Indexer transaction handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                txn_handler_exec_res = txn_handler.start().await;
            }
        });
        let event_handle = tokio::task::spawn(async move {
            let mut event_handler_exec_res = event_handler.start().await;
            while let Err(e) = event_handler_exec_res.clone() {
                warn!(
                    "Indexer event handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                event_handler_exec_res = event_handler.start().await;
            }
        });
        let object_event_handle = tokio::task::spawn(async move {
            let mut obj_event_handler_exec_res = obj_event_handler.start().await;
            while let Err(e) = obj_event_handler_exec_res.clone() {
                warn!(
                    "Indexer object event handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                obj_event_handler_exec_res = obj_event_handler.start().await;
            }
        });
        let publish_event_handle = tokio::task::spawn(async move {
            let mut publish_event_handler_exec_res = publish_event_handler.start().await;
            while let Err(e) = publish_event_handler_exec_res.clone() {
                warn!(
                    "Indexer publish event handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                publish_event_handler_exec_res = publish_event_handler.start().await;
            }
        });
        let move_event_handle = tokio::task::spawn(async move {
            let mut move_event_handler_exec_res = move_handler.start().await;
            while let Err(e) = move_event_handler_exec_res.clone() {
                warn!(
                    "Indexer move event handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                move_event_handler_exec_res = move_handler.start().await;
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
        .map_err(|e| {
            error!("Indexer handler orchestrator failed with error: {:?}", e);
            e
        })
        .expect("Handler orchestrator should not run into errors.");
    }
}
