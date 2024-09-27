// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use mysten_metrics::RegistryService;
use prometheus::{Registry, TextEncoder};
use std::net::TcpListener;
use std::sync::{Arc, RwLock};
use tower::ServiceBuilder;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;

const METRICS_ROUTE: &str = "/metrics";
const POD_HEALTH_ROUTE: &str = "/pod_health";

type HealthCheckMetrics = Arc<RwLock<HealthCheck>>;

/// Do not access struct members without using HealthCheckMetrics to arc+mutex
#[derive(Debug)]
struct HealthCheck {
    // eg; consumer_operations_submitted{...}
    consumer_operations_submitted: f64,
}

/// HealthCheck contains fields we believe are interesting that say whether this pod should be
/// considered health.  do not use w/o using an arc+mutex
impl HealthCheck {
    fn new() -> Self {
        Self {
            consumer_operations_submitted: 0.0,
        }
    }
}

// Creates a new http server that has as a sole purpose to expose
// and endpoint that prometheus agent can use to poll for the metrics.
// A RegistryService is returned that can be used to get access in prometheus Registries.
pub fn start_prometheus_server(listener: TcpListener) -> RegistryService {
    let registry = Registry::new();

    let registry_service = RegistryService::new(registry);

    let pod_health_data = Arc::new(RwLock::new(HealthCheck::new()));

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .route(POD_HEALTH_ROUTE, get(pod_health))
        .layer(Extension(registry_service.clone()))
        .layer(Extension(pod_health_data.clone()))
        .layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http().on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Seconds),
                ),
            ),
        );

    tokio::spawn(async move {
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    registry_service
}

// DO NOT remove this handler, it is not compatible with the mysten_metrics::metric equivalent
async fn metrics(
    Extension(registry_service): Extension<RegistryService>,
    Extension(pod_health): Extension<HealthCheckMetrics>,
) -> (StatusCode, String) {
    let mut metric_families = registry_service.gather_all();
    metric_families.extend(prometheus::gather());

    if let Some(consumer_operations_submitted) = metric_families
        .iter()
        .filter_map(|v| {
            if v.get_name() == "consumer_operations_submitted" {
                // Expecting one metric, so return the first one, as it is the only one
                v.get_metric().first().map(|m| m.get_counter().get_value())
            } else {
                None
            }
        })
        .next()
    {
        pod_health
            .write()
            .expect("unable to write to pod health metrics")
            .consumer_operations_submitted = consumer_operations_submitted;
    };
    match TextEncoder.encode_to_string(&metric_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}

/// pod_health is called by k8s to know if this service is correctly processing data
async fn pod_health(Extension(pod_health): Extension<HealthCheckMetrics>) -> (StatusCode, String) {
    let consumer_operations_submitted = pod_health
        .read()
        .expect("unable to read pod health metrics")
        .consumer_operations_submitted;

    if consumer_operations_submitted > 0.0 {
        (StatusCode::OK, consumer_operations_submitted.to_string())
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            consumer_operations_submitted.to_string(),
        )
    }
}
