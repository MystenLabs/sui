// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui::client_commands::WalletContext;
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::SuiClient;
use tracing::info;
pub mod fetcher;

use fetcher::event_handler::EventHandler;
use fetcher::transaction_handler::TransactionHandler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();
    info!("Sui indexer started...");
    let rpc_client = new_rpc_client().await?;
    let txn_handler = TransactionHandler::new(rpc_client.clone());
    let event_handler = EventHandler::new(rpc_client);

    tokio::task::spawn(async move {
        txn_handler.run_forever().await;
    });
    tokio::task::spawn(async move {
        event_handler.run_forever().await;
    });
    Ok(())
}

async fn new_rpc_client() -> Result<SuiClient, anyhow::Error> {
    info!("Getting new rpc client...");
    let wallet_conf = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let wallet_context = WalletContext::new(&wallet_conf, Some(Duration::from_secs(10))).await?;
    Ok(wallet_context.client)
}
