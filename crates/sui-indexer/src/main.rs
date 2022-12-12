// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use sui_indexer::errors::IndexerError;
use sui_indexer::{new_pg_connection_pool, new_rpc_client};

use backoff::future::retry;
use backoff::ExponentialBackoff;
use tracing::info;

use clap::Parser;

pub mod handlers;
pub mod processors;

use handlers::handler_orchestrator::HandlerOrchestrator;
use processors::processor_orchestrator::ProcessorOrchestrator;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();
    info!("Sui indexer started...");

    let indexer_config = IndexerConfig::parse();
    retry(ExponentialBackoff::default(), || async {
        let rpc_client = new_rpc_client(indexer_config.rpc_client_url.clone()).await?;
        let pg_connection_pool = new_pg_connection_pool(indexer_config.db_url.clone()).await?;
        // NOTE: Each handler is responsible for one type of data from nodes,like transactions and events;
        // Handler orchestrator runs these handlers in parallel and manage them upon errors etc.
        HandlerOrchestrator::new(rpc_client.clone(), pg_connection_pool.clone())
            .run_forever()
            .await;
        ProcessorOrchestrator::new(rpc_client.clone(), pg_connection_pool.clone())
            .run_forever()
            .await;

        Ok(())
    })
    .await
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
