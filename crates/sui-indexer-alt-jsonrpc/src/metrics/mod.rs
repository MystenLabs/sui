// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{http::StatusCode, routing::get, Extension, Router};
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry, HistogramVec,
    IntCounterVec, Registry, TextEncoder,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub(crate) mod middleware;

/// Histogram buckets for the distribution of latency (time between receiving a request and sending
/// a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0,
];

/// Service to expose prometheus metrics from the JSON-RPC service.
pub struct MetricsService {
    addr: SocketAddr,
    registry: Registry,
    cancel: CancellationToken,
}

#[derive(Clone)]
pub struct RpcMetrics {
    pub request_latency: HistogramVec,
    pub requests_received: IntCounterVec,
    pub requests_succeeded: IntCounterVec,
    pub requests_failed: IntCounterVec,
}

impl MetricsService {
    /// Create a new metrics service, exposing JSON-RPC-specific metrics. Returns the RPC-specific
    /// metrics and the service itself (which must be run with [Self::run]).
    pub(crate) fn new(
        addr: SocketAddr,
        cancel: CancellationToken,
    ) -> anyhow::Result<(Arc<RpcMetrics>, Self)> {
        let registry = Registry::new_custom(Some("jsonrpc_alt".to_string()), None)?;

        let metrics = RpcMetrics::new(&registry);

        let service = Self {
            addr,
            registry,
            cancel,
        };

        Ok((metrics, service))
    }

    /// Start the service. The service will run until the cancellation token is triggered.
    pub(crate) async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let listener = TcpListener::bind(&self.addr).await?;

        let app = Router::new()
            .route("/metrics", get(metrics))
            .layer(Extension(self.registry));

        Ok(tokio::spawn(async move {
            info!("Starting metrics service on {}", self.addr);
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    self.cancel.cancelled().await;
                    info!("Shutdown received, stopping metrics service");
                })
                .await
                .unwrap();
        }))
    }
}

impl RpcMetrics {
    fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            request_latency: register_histogram_vec_with_registry!(
                "rpc_request_latency",
                "Time taken to respond to JSON-RPC requests, by method",
                &["method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                "rpc_requests_received",
                "Number of requests initiated for each JSON-RPC method",
                &["method"],
                registry
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                "rpc_requests_succeeded",
                "Number of requests that completed successfully for each JSON-RPC method",
                &["method"],
                registry
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                "rpc_requests_failed",
                "Number of requests that completed with an error for each JSON-RPC method, by error code",
                &["method", "code"],
                registry
            )
            .unwrap(),
        })
    }
}

/// Route handler for metrics service
async fn metrics(Extension(registry): Extension<Registry>) -> (StatusCode, String) {
    match TextEncoder.encode_to_string(&registry.gather()) {
        Ok(s) => (StatusCode::OK, s),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {e}"),
        ),
    }
}
