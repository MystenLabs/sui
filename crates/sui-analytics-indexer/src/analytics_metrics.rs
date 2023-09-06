// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use mysten_metrics::RegistryService;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry, TextEncoder};
use std::net::SocketAddr;

const METRICS_ROUTE: &str = "/metrics";

#[derive(Clone)]
pub struct AnalyticsMetrics {
    pub total_checkpoint_received: IntCounter,
    pub total_transaction_received: IntCounter,
    pub total_transaction_object_received: IntCounter,
    pub total_object_received: IntCounter,
    pub total_event_received: IntCounter,
}

impl AnalyticsMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_checkpoint_received: register_int_counter_with_registry!(
                "total_checkpoint_received",
                "Total number of checkpoints received",
                registry,
            )
            .unwrap(),
            total_transaction_received: register_int_counter_with_registry!(
                "total_transaction_received",
                "Total number of transactions received",
                registry,
            )
            .unwrap(),
            total_transaction_object_received: register_int_counter_with_registry!(
                "total_transaction_object_received",
                "Total number of transaction objects received",
                registry,
            )
            .unwrap(),
            total_object_received: register_int_counter_with_registry!(
                "total_object_received",
                "Total number of objects received",
                registry,
            )
            .unwrap(),
            total_event_received: register_int_counter_with_registry!(
                "total_event_received",
                "Total number of events received",
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
