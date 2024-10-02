// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{routing::get, Extension, Router};
use config::{AuthorityIdentifier, WorkerId};
use mysten_metrics::{metrics, spawn_logged_monitored_task};
use mysten_network::multiaddr::Multiaddr;
use prometheus::{
    register_counter_with_registry, register_histogram_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Counter, Histogram,
    IntCounter, IntGauge, Registry,
};
use std::collections::HashMap;
use tokio::task::JoinHandle;

const METRICS_ROUTE: &str = "/metrics";
const PRIMARY_METRICS_PREFIX: &str = "narwhal_primary";
const WORKER_METRICS_PREFIX: &str = "narwhal_worker";

#[derive(Clone)]
pub struct NarwhalBenchMetrics {
    pub narwhal_benchmark_duration: IntGauge,
    pub narwhal_client_num_success: IntCounter,
    pub narwhal_client_num_error: IntCounter,
    pub narwhal_client_num_submitted: IntCounter,
    pub narwhal_client_latency_s: Histogram,
    pub narwhal_client_latency_squared_s: Counter,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl NarwhalBenchMetrics {
    pub fn new(registry: &Registry) -> Self {
        NarwhalBenchMetrics {
            narwhal_benchmark_duration: register_int_gauge_with_registry!(
                "narwhal_benchmark_duration",
                "Duration of the benchmark",
                registry,
            )
            .unwrap(),
            narwhal_client_num_success: register_int_counter_with_registry!(
                "narwhal_client_num_success",
                "Total number of transaction success",
                registry,
            )
            .unwrap(),
            narwhal_client_num_error: register_int_counter_with_registry!(
                "narwhal_client_num_error",
                "Total number of transaction errors",
                registry,
            )
            .unwrap(),
            narwhal_client_num_submitted: register_int_counter_with_registry!(
                "narwhal_client_num_submitted",
                "Total number of transaction submitted to narwhal",
                registry,
            )
            .unwrap(),
            narwhal_client_latency_s: register_histogram_with_registry!(
                "narwhal_client_latency_s",
                "Total time in seconds to return a response",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            narwhal_client_latency_squared_s: register_counter_with_registry!(
                "narwhal_client_latency_squared_s",
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
            let listener = tokio::net::TcpListener::bind(&socket_addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        },
        "MetricsServerTask"
    )
}
