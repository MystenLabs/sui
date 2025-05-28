// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{routing::any, Router};
use clap::Parser;
use mysten_metrics::start_prometheus_server;
use reqwest::Client;
use std::sync::Arc;
use sui_indexer_alt_jsonrpc_proxy::config::{load, ProxyConfig};
use sui_indexer_alt_jsonrpc_proxy::cursor::PaginationCursorState;
use sui_indexer_alt_jsonrpc_proxy::handlers::{proxy_handler, AppState};
use sui_indexer_alt_jsonrpc_proxy::metrics::AppMetrics;
use tracing::info;

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(
        long,
        short,
        default_value = "./config.yaml",
        help = "Specify the config file path to use"
    )]
    config: String,
}

#[tokio::main]
async fn main() {
    info!("Starting sui-indexer-alt-jsonrpc-proxy");
    let args = Args::parse();

    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new().with_env().init();
    info!("Initialized telemetry");

    let (config, client): (ProxyConfig, Client) =
        load(&args.config).await.expect("Failed to load config");
    info!("Loaded config: {:?}", config);

    let registry_service = start_prometheus_server(config.metrics_address);
    info!("Started prometheus server at {}", config.metrics_address);

    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    info!("Initialized metrics");

    let app_metrics = AppMetrics::new(&prometheus_registry);
    info!("Created app metrics");

    // Create a single shared cursor state
    let cursor_state = Arc::new(PaginationCursorState::new(config.cursor_cache_size));
    info!("Created cursor state");

    let app_state = AppState::new(
        client,
        config.fullnode_address,
        config.unsupported_methods.into_iter().collect(),
        cursor_state,
        app_metrics,
    );
    info!("Created app state");

    let app = Router::new()
        .fallback(any(proxy_handler))
        .with_state(app_state);

    info!("Starting server on {}", config.listen_address);
    axum_server::Server::bind(config.listen_address)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
