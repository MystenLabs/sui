// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer::errors::IndexerError;
use sui_indexer::{new_pg_connection_pool, new_rpc_client};
use sui_node::metrics::start_prometheus_server;

use backoff::future::retry;
use backoff::ExponentialBackoff;
use futures::future::try_join_all;
use tracing::info;

use clap::Parser;

pub mod handlers;
pub mod processors;

use handlers::handler_orchestrator::HandlerOrchestrator;
use processors::processor_orchestrator::ProcessorOrchestrator;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    info!("Sui indexer started...");

    let indexer_config = IndexerConfig::parse();
    retry(ExponentialBackoff::default(), || async {
        let rpc_client = new_rpc_client(indexer_config.rpc_client_url.clone()).await?;
        let pg_connection_pool = new_pg_connection_pool(indexer_config.db_url.clone()).await?;
        // NOTE: Each handler is responsible for one type of data from nodes,like transactions and events;
        // Handler orchestrator runs these handlers in parallel and manage them upon errors etc.
        let handler_rpc_client = rpc_client.clone();
        let handler_pg_pool = pg_connection_pool.clone();

        let registry_service = start_prometheus_server(
            // NOTE: this parses the input host addr and port number for socket addr,
            // so unwrap() is safe here.
            format!(
                "{}:{}",
                indexer_config.client_metric_host, indexer_config.client_metric_port
            )
            .parse()
            .unwrap(),
        );
        let prometheus_registry = registry_service.default_registry();
        let handler_prometheus_registry = prometheus_registry.clone();
        let handler_handle = tokio::spawn(async move {
            HandlerOrchestrator::new(
                handler_rpc_client,
                handler_pg_pool,
                handler_prometheus_registry,
            )
            .run_forever()
            .await;
        });

        let processor_prometheus_registry = prometheus_registry.clone();
        let processor_handle = tokio::spawn(async move {
            ProcessorOrchestrator::new(
                rpc_client.clone(),
                pg_connection_pool.clone(),
                processor_prometheus_registry,
            )
            .run_forever()
            .await;
        });

        try_join_all(vec![handler_handle, processor_handle])
            .await
            .expect("Indexer main should not run into errors.");
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
    #[clap(long, default_value = "0.0.0.0", global = true)]
    pub client_metric_host: String,
    #[clap(long, default_value = "9184", global = true)]
    pub client_metric_port: u16,
}
