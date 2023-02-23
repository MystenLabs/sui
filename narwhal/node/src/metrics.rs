// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{http::StatusCode, routing::get, Extension, Router};
use config::WorkerId;
use crypto::PublicKey;
use multiaddr::Multiaddr;
use mysten_network::multiaddr::to_socket_addr;
use prometheus::{Registry, TextEncoder};
use std::collections::HashMap;
use tokio::task::JoinHandle;

const METRICS_ROUTE: &str = "/metrics";
const PRIMARY_METRICS_PREFIX: &str = "narwhal_primary";
const WORKER_METRICS_PREFIX: &str = "narwhal_worker";

pub fn new_registry() -> Registry {
    Registry::new_custom(None, None).unwrap()
}

pub fn primary_metrics_registry(authority_id: AuthorityIdentifier) -> Registry {
    let mut labels = HashMap::new();
    labels.insert("node_name".to_string(), authority_id.to_string());

    Registry::new_custom(Some(PRIMARY_METRICS_PREFIX.to_string()), Some(labels)).unwrap()
}

pub fn worker_metrics_registry(worker_id: WorkerId, authority_id: AuthorityIdentifier) -> Registry {
    let mut labels = HashMap::new();
    labels.insert("node_name".to_string(), authority_id.to_string());
    labels.insert("worker_id".to_string(), worker_id.to_string());

    Registry::new_custom(Some(WORKER_METRICS_PREFIX.to_string()), Some(labels)).unwrap()
}

#[must_use]
pub fn start_prometheus_server(addr: Multiaddr, registry: &Registry) -> JoinHandle<()> {
    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry.clone()));

    let socket_addr = to_socket_addr(&addr).expect("failed to convert Multiaddr to SocketAddr");

    tokio::spawn(async move {
        axum::Server::bind(&socket_addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    })
}

async fn metrics(registry: Extension<Registry>) -> (StatusCode, String) {
    let metrics_families = registry.gather();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}
