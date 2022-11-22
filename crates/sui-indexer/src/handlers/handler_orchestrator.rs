// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_sdk::SuiClient;
use tracing::info;

use crate::handlers::event_handler::EventHandler;
use crate::handlers::transaction_handler::TransactionHandler;

// NOCOMMIT
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone)]
pub struct HandlerOrchestrator {
    rpc_client: SuiClient,
}

impl HandlerOrchestrator {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self { rpc_client }
    }

    pub async fn run_forever(&self) {
        let txn_handler = TransactionHandler::new(self.rpc_client.clone());
        let event_handler = EventHandler::new(self.rpc_client.clone());
        info!("Handler orchestrator started...");

        // TDDOggao: manage errors from handler
        tokio::task::spawn(async move {
            txn_handler.start().await;
        });
        tokio::task::spawn(async move {
            event_handler.start().await;
        });

        // NOCOMMIT
        sleep(Duration::from_secs(20)).await;
    }
}
