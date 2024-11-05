// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use axum::{
    routing::{any, get},
    Router,
};
use clap::Parser;
use mysten_metrics::start_prometheus_server;
use sui_edge_proxy::config::{load, ProxyConfig};
use sui_edge_proxy::handlers::{health_check_handler, proxy_handler, AppState};
use sui_edge_proxy::metrics::AppMetrics;
use tracing::info;

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(
        long,
        short,
        default_value = "./sui-edge-proxy.yaml",
        help = "Specify the config file path to use"
    )]
    config: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config: ProxyConfig = load(&args.config).unwrap();

    let registry_service = start_prometheus_server(config.metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);

    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    info!("Metrics server started at {}", config.metrics_address);

    // Build a reqwest client that supports HTTP/2
    let client = reqwest::ClientBuilder::new()
        .http2_prior_knowledge()
        .http2_keep_alive_while_idle(true)
        .pool_idle_timeout(None)
        .pool_max_idle_per_host(config.max_idle_connections.unwrap_or(100))
        .build()
        .unwrap();

    let app_metrics = AppMetrics::new(&prometheus_registry);

    let app_state = AppState::new(
        client,
        config.read_peer.clone(),
        config.execution_peer.clone(),
        app_metrics,
    );

    let app = Router::new()
        .route("/health", get(health_check_handler))
        .fallback(any(proxy_handler))
        .with_state(app_state);

    info!("Starting server on {}", config.listen_address);
    axum_server::Server::bind(config.listen_address)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
