// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use prometheus::Histogram;
use prometheus::HistogramTimer;
use prometheus::HistogramVec;
use prometheus::IntCounter;
use prometheus::IntCounterVec;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_histogram_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::register_int_counter_with_registry;
use tower::Layer;
use tower::Service;

/// Histogram buckets for the distribution of latency (time between sending a DB request and
/// receiving a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

#[derive(Clone)]
pub(crate) struct DbReaderMetrics {
    pub latency: Histogram,
    pub requests_received: IntCounter,
    pub requests_succeeded: IntCounter,
    pub requests_failed: IntCounter,
}

#[derive(Clone)]
pub(crate) struct ConsistentReaderMetrics {
    pub latency: HistogramVec,
    pub requests_received: IntCounterVec,
    pub requests_succeeded: IntCounterVec,
    pub requests_failed: IntCounterVec,
}

#[derive(Clone)]
pub struct GrpcMetrics {
    latency: HistogramVec,
    requests_received: IntCounterVec,
    requests_succeeded: IntCounterVec,
    requests_failed: IntCounterVec,
    requests_cancelled: IntCounterVec,
}

#[derive(Clone)]
pub struct GrpcMetricsLayer {
    metrics: Arc<GrpcMetrics>,
}

/// Middleware to record metrics defined in `GrpcMetrics`
#[derive(Clone)]
pub struct GrpcMetricsService<S> {
    inner: S,
    metrics: Arc<GrpcMetrics>,
}

struct MetricsGuard {
    metrics: Arc<GrpcMetrics>,
    method: String,
    timer: Option<HistogramTimer>,
}

impl DbReaderMetrics {
    pub(crate) fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("db");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            latency: register_histogram_with_registry!(
                name("latency"),
                "Time taken by the database to respond to queries",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            requests_received: register_int_counter_with_registry!(
                name("requests_received"),
                "Number of database requests sent by the service",
                registry,
            )
            .unwrap(),

            requests_succeeded: register_int_counter_with_registry!(
                name("requests_succeeded"),
                "Number of database requests that completed successfully",
                registry,
            )
            .unwrap(),

            requests_failed: register_int_counter_with_registry!(
                name("requests_failed"),
                "Number of database requests that completed with an error",
                registry,
            )
            .unwrap(),
        })
    }
}

impl ConsistentReaderMetrics {
    pub(crate) fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("consistent_store");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            latency: register_histogram_vec_with_registry!(
                name("latency"),
                "Time taken by the consistent store to respond to queries",
                &["method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                name("requests_received"),
                "Number of consistent store requests sent by the service",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                name("requests_succeeded"),
                "Number of consistent store requests that completed successfully",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                name("requests_failed"),
                "Number of consistent store requests that completed with an error",
                &["method"],
                registry,
            )
            .unwrap(),
        })
    }
}

impl GrpcMetrics {
    pub fn new(prefix: &str, registry: &Registry) -> Self {
        let name = |n| format!("{prefix}_{n}");

        Self {
            latency: register_histogram_vec_with_registry!(
                name("latency"),
                "Time taken for gRPC operations",
                &["method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                name("requests_received"),
                "Number of gRPC requests sent",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                name("requests_succeeded"),
                "Number of gRPC requests that completed successfully",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                name("requests_failed"),
                "Number of gRPC requests that completed with an error",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_cancelled: register_int_counter_vec_with_registry!(
                name("requests_cancelled"),
                "Number of gRPC requests dropped before completion (cancellation or panic)",
                &["method"],
                registry,
            )
            .unwrap(),
        }
    }
}

impl GrpcMetricsLayer {
    pub fn new(prefix: &str, registry: &Registry) -> Self {
        Self {
            metrics: Arc::new(GrpcMetrics::new(prefix, registry)),
        }
    }
}

impl MetricsGuard {
    fn record(&mut self, ok: bool) {
        self.timer.take();
        let counter = if ok {
            &self.metrics.requests_succeeded
        } else {
            &self.metrics.requests_failed
        };
        counter.with_label_values(&[&self.method]).inc();
    }
}

impl<S> Layer<S> for GrpcMetricsLayer {
    type Service = GrpcMetricsService<S>;

    fn layer(&self, service: S) -> Self::Service {
        GrpcMetricsService {
            inner: service,
            metrics: self.metrics.clone(),
        }
    }
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for GrpcMetricsService<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let method = req.uri().path().to_string();

        let metrics = self.metrics.clone();
        metrics
            .requests_received
            .with_label_values(&[&method])
            .inc();
        let timer = metrics.latency.with_label_values(&[&method]).start_timer();

        let fut = self.inner.call(req);

        let mut guard = MetricsGuard {
            metrics,
            method,
            timer: Some(timer),
        };

        Box::pin(async move {
            let result = fut.await;
            guard.record(result.is_ok());
            result
        })
    }
}

impl Drop for MetricsGuard {
    fn drop(&mut self) {
        if self.timer.is_some() {
            self.metrics
                .requests_cancelled
                .with_label_values(&[&self.method])
                .inc();
        }
    }
}
