// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use mysten_network::metrics::MetricsCallbackProvider;
use prometheus::{register_int_gauge_vec_with_registry, IntGaugeVec, Registry, TextEncoder};
use std::net::SocketAddr;
use std::time::Duration;
use sui_network::tonic::Code;

const METRICS_ROUTE: &str = "/metrics";

pub fn start_prometheus_server(addr: SocketAddr) -> Registry {
    let registry = Registry::new();

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry.clone()));

    tokio::spawn(async move {
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    registry
}

async fn metrics(Extension(registry): Extension<Registry>) -> (StatusCode, String) {
    let metrics_families = registry.gather();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}

#[derive(Clone)]
pub struct GrpcMetrics {
    inflight_grpc: IntGaugeVec,
}

impl GrpcMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_grpc: register_int_gauge_vec_with_registry!(
                "inflight_grpc",
                "Total in-flight GRPC per route",
                &["path"],
                registry,
            )
            .unwrap(),
        }
    }
}

impl MetricsCallbackProvider for GrpcMetrics {
    fn on_request(&self, path: String) {
        self.inflight_grpc.with_label_values(&[&path]).inc();
    }

    fn on_response(&self, path: String, _latency: Duration, _status: u16, _grpc_status_code: Code) {
        self.inflight_grpc.with_label_values(&[&path]).dec();
    }
}
