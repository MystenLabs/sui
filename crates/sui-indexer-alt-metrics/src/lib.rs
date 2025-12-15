// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, time::Instant};

use anyhow::Context;
use axum::{Extension, Router, http::StatusCode, routing::get};
use prometheus::{Registry, TextEncoder, core::Collector};
use prometheus_closure_metric::{ClosureMetric, ValueType};
use sui_futures::service::Service;
use tokio::{net::TcpListener, sync::oneshot};
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
}

impl MetricsService {
    /// Create a new instance of the service, listening on the address provided in `args`, serving
    /// metrics from the `registry`.
    ///
    /// The service will not be run until [Self::run] is called.
    pub fn new(args: MetricsArgs, registry: Registry) -> Self {
        Self {
            addr: args.metrics_address,
            registry,
        }
    }

    /// Add metrics to this registry to serve them from this service.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Start the service. The service will run until the cancellation token is triggered.
    pub async fn run(self) -> anyhow::Result<Service> {
        let Self { addr, registry } = self;

        let listener = TcpListener::bind(&self.addr)
            .await
            .with_context(|| format!("Failed to bind metrics at {addr}"))?;

        let app = Router::new()
            .route("/metrics", get(metrics))
            .layer(Extension(registry));

        let (stx, srx) = oneshot::channel::<()>();
        Ok(Service::new()
            .with_shutdown_signal(async move {
                let _ = stx.send(());
            })
            .spawn(async move {
                info!("Starting metrics service on {addr}");
                Ok(axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = srx.await;
                        info!("Shutdown received, shutting down metrics service");
                    })
                    .await?)
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

/// A metric that tracks the service uptime.
pub fn uptime(version: &str) -> anyhow::Result<Box<dyn Collector>> {
    let init = Instant::now();
    let opts = prometheus::opts!("uptime", "how long the service has been running in seconds")
        .variable_label("version");

    let metric = move || init.elapsed().as_secs();
    let uptime = ClosureMetric::new(opts, ValueType::Counter, metric, &[version])
        .context("Failed to create uptime metric")?;

    Ok(Box::new(uptime))
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
