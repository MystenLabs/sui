// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use axum::{http::StatusCode, routing::get, Extension, Router};
use prometheus::{Registry, TextEncoder};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod db;

#[derive(clap::Args, Debug, Clone)]
pub struct MetricsArgs {
    /// Address to serve Prometheus metrics from.
    #[arg(long, default_value_t = Self::default().metrics_address)]
    pub metrics_address: SocketAddr,
}

/// A service that exposes prometheus metrics over HTTP on a "/metrics" route on the provided
/// listen address.
pub struct MetricsService {
    addr: SocketAddr,
    registry: Registry,
    cancel: CancellationToken,
}

impl MetricsService {
    /// Create a new instance of the service, listening on the address provided in `args`, serving
    /// metrics from the `registry`. The service will shut down if the provided `cancel` token is
    /// cancelled.
    ///
    /// The service will not be run until [Self::run] is called.
    pub fn new(args: MetricsArgs, registry: Registry, cancel: CancellationToken) -> Self {
        Self {
            addr: args.metrics_address,
            registry,
            cancel,
        }
    }

    /// Add metrics to this registry to serve them from this service.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Start the service. The service will run until the cancellation token is triggered.
    pub async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let Self {
            addr,
            registry,
            cancel,
        } = self;

        let listener = TcpListener::bind(&self.addr).await?;
        let app = Router::new()
            .route("/metrics", get(metrics))
            .layer(Extension(registry));

        Ok(tokio::spawn(async move {
            info!("Starting metrics service on {}", addr);
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    cancel.cancelled().await;
                    info!("Shutdown received, shutting down metrics service");
                })
                .await
                .unwrap()
        }))
    }
}

impl Default for MetricsArgs {
    fn default() -> Self {
        Self {
            metrics_address: "0.0.0.0:9184".parse().unwrap(),
        }
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
