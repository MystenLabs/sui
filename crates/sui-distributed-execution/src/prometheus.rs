// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{http::StatusCode, routing::get, Extension, Router, Server};
use prometheus::{Registry, TextEncoder};
use std::net::SocketAddr;
use tokio::task::JoinHandle;

pub const METRICS_ROUTE: &str = "/metrics";

pub fn start_prometheus_server(
    address: SocketAddr,
    registry: &Registry,
) -> JoinHandle<Result<(), hyper::Error>> {
    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry.clone()));

    tracing::info!("Prometheus server booted on {address}");
    tokio::spawn(async move { Server::bind(&address).serve(app.into_make_service()).await })
}

async fn metrics(registry: Extension<Registry>) -> (StatusCode, String) {
    let metrics_families = registry.gather();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unable to encode metrics: {error}"),
        ),
    }
}
