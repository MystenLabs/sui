// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use sui_sdk::SuiClient;
use tracing::{error, info};

use crate::handlers::event_handler::EventHandler;
use crate::handlers::transaction_handler::TransactionHandler;

#[derive(Clone)]
pub struct HandlerOrchestrator {
    rpc_client: SuiClient,
}

impl HandlerOrchestrator {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self { rpc_client }
    }

    pub async fn run_forever(&self) {
        let event_handler = EventHandler::new(self.rpc_client.clone());
        let txn_handler = TransactionHandler::new(self.rpc_client.clone());
        info!("Handler orchestrator started...");

        tokio::task::spawn(async move {
            let txn_res = retry(ExponentialBackoff::default(), || async {
                Ok(txn_handler.start().await?)
            })
            .await;
            if let Err(e) = txn_res {
                error!(
                    "Indexer transaction handler failed after retrials with error: {:?}!",
                    e
                );
            }
        });
        tokio::task::spawn(async move {
            let event_res = retry(ExponentialBackoff::default(), || async {
                Ok(event_handler.start().await?)
            })
            .await;
            if let Err(e) = event_res {
                error!(
                    "Indexer event handler failed after retrials with error: {:?}",
                    e
                );
            }
        });
    }
}
