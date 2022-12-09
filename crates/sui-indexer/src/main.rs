// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use sui_sdk::SuiClient;
use tracing::info;

use clap::Parser;
use std::env;

pub mod handlers;
pub mod processors;

use handlers::handler_orchestrator::HandlerOrchestrator;
use processors::processor_orchestrator::ProcessorOrchestrator;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();
    info!("Sui indexer started...");

    let indexer_config = IndexerConfig::parse();
    retry(ExponentialBackoff::default(), || async {
        let rpc_client = new_rpc_client(indexer_config.rpc_client_url.clone()).await?;
        // NOTE: Each handler is responsible for one type of data from nodes,like transactions and events;
        // Handler orchestrator runs these handlers in parallel and manage them upon errors etc.
        HandlerOrchestrator::new(rpc_client.clone(), indexer_config.db_url.clone())
            .run_forever()
            .await;
        ProcessorOrchestrator::new(rpc_client.clone(), indexer_config.db_url.clone())
            .run_forever()
            .await;
        Ok(())
    })
    .await
}

async fn new_rpc_client(http_url: String) -> Result<SuiClient, anyhow::Error> {
    info!("Getting new rpc client...");
    let rpc_client = SuiClient::new(http_url.as_str(), None, None).await?;
    Ok(rpc_client)
}

#[derive(Parser)]
#[clap(
    name = "Sui indexer",
    about = "An off-fullnode service serving data from Sui protocol",
    rename_all = "kebab-case"
)]
struct IndexerConfig {
    #[clap(long)]
    db_url: String,
    #[clap(long)]
    rpc_client_url: String,
}
