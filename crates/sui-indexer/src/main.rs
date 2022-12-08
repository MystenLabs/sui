// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use backoff::future::retry;
use backoff::ExponentialBackoff;
use std::time::Duration;
use sui::config::{PersistedConfig, SuiClientConfig};
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::SuiClient;
use tracing::info;

use dotenvy::dotenv;
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

    retry(ExponentialBackoff::default(), || async {
        let rpc_client = new_rpc_client().await?;
        dotenv().ok();
        let database_url =
            env::var("DATABASE_URL").expect("DATABASE_URL must be set in env. to start indexer.");
        // NOTE: Each handler is responsible for one type of data from nodes,like transactions and events;
        // Handler orchestrator runs these handlers in parallel and manage them upon errors etc.
        HandlerOrchestrator::new(rpc_client.clone(), database_url.clone())
            .run_forever()
            .await;
        ProcessorOrchestrator::new(rpc_client.clone(), database_url)
            .run_forever()
            .await;
        Ok(())
    })
    .await
}

async fn new_rpc_client() -> Result<SuiClient, anyhow::Error> {
    info!("Getting new rpc client...");
    let config_path = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    let config: SuiClientConfig = PersistedConfig::read(&config_path)?;
    config
        .get_active_env()?
        .create_rpc_client(Some(Duration::from_secs(10)))
        .await
}
