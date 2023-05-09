// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use mysten_metrics::RegistryService;
use prometheus::{Registry, TextEncoder};
use std::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;

const METRICS_ROUTE: &str = "/metrics";

// Creates a new http server that has as a sole purpose to expose
// and endpoint that prometheus agent can use to poll for the metrics.
// A RegistryService is returned that can be used to get access in prometheus Registries.
pub fn start_prometheus_server(addr: TcpListener) -> RegistryService {
    let registry = Registry::new();

    let registry_service = RegistryService::new(registry);

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry_service.clone()))
        .layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http().on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Seconds),
                ),
            ),
        );

    tokio::spawn(async move {
        axum::Server::from_tcp(addr)
            .unwrap()
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    registry_service
}

async fn metrics(Extension(registry_service): Extension<RegistryService>) -> (StatusCode, String) {
    let mut metric_families = registry_service.gather_all();
    metric_families.extend(prometheus::gather());
    match TextEncoder.encode_to_string(&metric_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}
