// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{routing::get, Extension, Router};
use config::{AuthorityIdentifier, WorkerId};
use mysten_metrics::{metrics, spawn_logged_monitored_task};
use mysten_network::multiaddr::Multiaddr;
use prometheus::Registry;
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

    let socket_addr = addr
        .to_socket_addr()
        .expect("failed to convert Multiaddr to SocketAddr");

    spawn_logged_monitored_task!(
        async move {
            axum::Server::bind(&socket_addr)
                .serve(app.into_make_service())
                .await
                .unwrap();
        },
        "MetricsServerTask"
    )
}
