// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use sui_sdk::SuiClient;
use tracing::{error, info, warn};

use crate::handlers::event_handler::EventHandler;
use crate::handlers::transaction_handler::TransactionHandler;

#[derive(Clone)]
pub struct HandlerOrchestrator {
    rpc_client: SuiClient,
    db_url: String,
}

impl HandlerOrchestrator {
    pub fn new(rpc_client: SuiClient, db_url: String) -> Self {
        Self { rpc_client, db_url }
    }

    pub async fn run_forever(&self) {
        info!("Handler orchestrator started...");
        let event_handler = EventHandler::new(self.rpc_client.clone(), self.db_url.clone());
        let txn_handler = TransactionHandler::new(self.rpc_client.clone(), self.db_url.clone());

        tokio::task::spawn(async move {
            let txn_res = retry(ExponentialBackoff::default(), || async {
                let txn_handler_exec_res = txn_handler.start().await;
                if let Err(e) = txn_handler_exec_res.clone() {
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
        tokio::task::spawn(async move {
            let event_res = retry(ExponentialBackoff::default(), || async {
                let event_handler_exec_res = event_handler.start().await;
                if let Err(e) = event_handler_exec_res.clone() {
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
    }
}
