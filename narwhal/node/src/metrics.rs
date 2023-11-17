// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{routing::get, Extension, Router};
use config::{AuthorityIdentifier, WorkerId};
use mysten_metrics::{metrics, spawn_logged_monitored_task};
use mysten_network::multiaddr::Multiaddr;
use prometheus::{
    register_counter_with_registry, register_histogram_with_registry,
    register_int_counter_with_registry, Counter, Histogram, IntCounter, Registry,
};
use std::collections::HashMap;
use tokio::task::JoinHandle;

const METRICS_ROUTE: &str = "/metrics";
const PRIMARY_METRICS_PREFIX: &str = "narwhal_primary";
const WORKER_METRICS_PREFIX: &str = "narwhal_worker";

pub struct BenchMetrics {
    pub benchmark_duration: IntCounter,
    pub num_success: IntCounter,
    pub num_error: IntCounter,
    pub num_submitted: IntCounter,
    pub latency_s: Histogram,
    pub latency_squared_s: Counter,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl BenchMetrics {
    pub fn new(registry: &Registry) -> Self {
        BenchMetrics {
            benchmark_duration: register_int_counter_with_registry!(
                "benchmark_duration",
                "Duration of the benchmark",
                registry,
            )
            .unwrap(),
            num_success: register_int_counter_with_registry!(
                "num_success",
                "Total number of transaction success",
                registry,
            )
            .unwrap(),
            num_error: register_int_counter_with_registry!(
                "num_error",
                "Total number of transaction errors",
                registry,
            )
            .unwrap(),
            num_submitted: register_int_counter_with_registry!(
                "num_submitted",
                "Total number of transaction submitted to narwhal",
                registry,
            )
            .unwrap(),
            latency_s: register_histogram_with_registry!(
                "latency_s",
                "Total time in seconds to return a response",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            latency_squared_s: register_counter_with_registry!(
                "latency_squared_s",
                "Square of total time in seconds to return a response",
                registry,
            )
            .unwrap(),
        }
    }
}

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
