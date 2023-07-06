// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use clap::Parser;
use mysten_metrics::RegistryService;
use prometheus::{Registry, TextEncoder};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::Config;
use sui_oracle::{config::OracleNodeConfig, OracleNode};
use sui_sdk::wallet_context::WalletContext;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
struct Args {
    #[clap(long)]
    pub oracle_config_path: PathBuf,
    #[clap(long)]
    pub client_config_path: PathBuf,
}

const METRICS_ROUTE: &str = "/metrics";
pub fn start_prometheus_server(addr: SocketAddr) -> RegistryService {
    let registry = Registry::new();

    let registry_service = RegistryService::new(registry);

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry_service.clone()));

    tokio::spawn(async move {
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    registry_service
}

// TODO dedup this function and move to mysten-metrics
async fn metrics(Extension(registry_service): Extension<RegistryService>) -> (StatusCode, String) {
    let metrics_families = registry_service.gather_all();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = OracleNodeConfig::load(&args.oracle_config_path)?;

    let wallet_ctx = WalletContext::new(
        &args.client_config_path,
        // TODO make this configurable
        Some(Duration::from_secs(10)), // request times out after 10 secs
        None,
    )
    .await?;

    // Init metrics server
    let registry_service = start_prometheus_server(config.metrics_address);
    let prometheus_registry = registry_service.default_registry();

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    OracleNode::new(
        config.upload_feeds,
        config.gas_object_id,
        config.download_feeds,
        wallet_ctx,
        prometheus_registry,
    )
    .run()
    .await?;

    Ok(())
}
