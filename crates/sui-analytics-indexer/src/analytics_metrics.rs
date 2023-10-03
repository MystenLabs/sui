// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::net::SocketAddr;

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry, TextEncoder,
};

use mysten_metrics::RegistryService;

const METRICS_ROUTE: &str = "/metrics";

#[derive(Clone)]
pub struct AnalyticsMetrics {
    pub total_received: IntCounterVec,
    pub last_uploaded_checkpoint: IntGaugeVec,
}

impl AnalyticsMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_received: register_int_counter_vec_with_registry!(
                "total_received",
                "Number of checkpoints received",
                &["data_type"],
                registry
            )
            .unwrap(),
            last_uploaded_checkpoint: register_int_gauge_vec_with_registry!(
                "last_uploaded_checkpoint",
                "Number of uploaded checkpoints.",
                &["data_type"],
                registry,
            )
            .unwrap(),
        }
    }
}

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
