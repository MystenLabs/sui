// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use axum::extract::State;
use axum::routing::get;
use mysten_service::get_mysten_service;
use mysten_service::metrics::start_basic_prometheus_server;
use mysten_service::package_name;
use mysten_service::package_version;
use mysten_service::serve;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use tracing::debug;

/// This is your app state to pass around to handlers
#[derive(Clone)]
struct AppState {
    /// application metrics
    metrics: MyMetrics,
}

#[derive(Clone)]
pub struct MyMetrics {
    pub requests: IntCounter,
}

impl MyMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            requests: register_int_counter_with_registry!(
                "total_requests",
                "Total number of requests received by my service",
                registry
            )
            .unwrap(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // set up logging
    // note: this guard needs to not be dropped until the process exits
    // see http://tinyurl.com/34jsvyc4 for more
    let _guard = mysten_service::logging::init();
    debug!("logging set up, setting up metrics");

    // initialize metrics
    let registry = start_basic_prometheus_server();
    // hook up custom application metrics
    let metrics = MyMetrics::new(&registry);
    debug!("metrics set up, starting service");

    let state = AppState { metrics };
    let app = get_mysten_service(package_name!(), package_version!())
        // this is just an axum router – add your own routes here
        .route("/example", get(hello))
        // attach app state so that handlers can publish metrics
        .with_state(state);
    serve(app).await
}

/// basic handler that responds with a static string
async fn hello(State(app_state): State<AppState>) -> &'static str {
    app_state.metrics.requests.inc();
    "Hello, World!"
}
