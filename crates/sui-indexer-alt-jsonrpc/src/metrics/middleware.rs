// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Cow,
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use jsonrpsee::{
    server::middleware::rpc::RpcServiceT,
    types::{error::INTERNAL_ERROR_CODE, Request},
    MethodResponse,
};
use pin_project::{pin_project, pinned_drop};
use prometheus::{HistogramTimer, IntCounterVec};
use serde_json::value::RawValue;
use tower_layer::Layer;
use tracing::{debug, error, info, warn};

use super::RpcMetrics;

/// Tower Layer that adds middleware to record statistics about RPC requests (how long they took to
/// serve, how many we have served, how many succeeded or failed, etc).
#[derive(Clone)]
pub(crate) struct MetricsLayer {
    metrics: Arc<RpcMetrics>,
    methods: Arc<HashSet<String>>,
    slow_request_threshold: Duration,
}

/// The Tower Service responsible for wrapping the JSON-RPC request handler with metrics handling.
pub(crate) struct MetricsService<S> {
    layer: MetricsLayer,
    inner: S,
}

struct RequestMetrics {
    timer: HistogramTimer,
    succeeded: IntCounterVec,
    failed: IntCounterVec,
    cancelled: IntCounterVec,
}

#[pin_project(PinnedDrop)]
pub(crate) struct MetricsFuture<'a, F> {
    metrics: Option<RequestMetrics>,
    method: Cow<'a, str>,
    // RPC request params for logging
    params: Option<Cow<'a, RawValue>>,
    slow_request_threshold: Duration,
    #[pin]
    inner: F,
}

impl MetricsLayer {
    /// Create a new metrics layer that only records statistics for the given methods (any other
    /// methods will be replaced with "<UNKNOWN>").
    pub fn new(
        metrics: Arc<RpcMetrics>,
        methods: HashSet<String>,
        slow_request_threshold: Duration,
    ) -> Self {
        Self {
            metrics,
            methods: Arc::new(methods),
            slow_request_threshold,
        }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsService {
            layer: self.clone(),
            inner,
        }
    }
}

impl<'a, S> RpcServiceT<'a> for MetricsService<S>
where
    S: RpcServiceT<'a>,
{
    type Future = MetricsFuture<'a, S::Future>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let method = if self.layer.methods.contains(request.method_name()) {
            request.method.clone()
        } else {
            // TODO(DVX-1210): the request method name is only here to make sure we know
            // about all the methods called by first party apps. We should change
            // this back to "<UNKNOWN>" once we have stabilized the API to avoid
            // high cardinality of metrics labels.
            format!("UNKNOWN:{}", request.method_name()).into()
        };

        self.layer
            .metrics
            .requests_received
            .with_label_values(&[method.as_ref()])
            .inc();

        let timer = self
            .layer
            .metrics
            .request_latency
            .with_label_values(&[method.as_ref()])
            .start_timer();

        MetricsFuture {
            metrics: Some(RequestMetrics {
                timer,
                succeeded: self.layer.metrics.requests_succeeded.clone(),
                failed: self.layer.metrics.requests_failed.clone(),
                cancelled: self.layer.metrics.requests_cancelled.clone(),
            }),
            method,
            params: request.params.clone(),
            slow_request_threshold: self.layer.slow_request_threshold,
            inner: self.inner.call(request),
        }
    }
}

impl<F> Future for MetricsFuture<'_, F>
where
    F: Future<Output = MethodResponse>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let Poll::Ready(resp) = this.inner.poll(cx) else {
            return Poll::Pending;
        };

        let Some(metrics) = this.metrics.take() else {
            return Poll::Ready(resp);
        };

        let method = this.method.as_ref();
        let elapsed_ms = metrics.timer.stop_and_record() * 1000.0;
        let slow_threshold_ms = this.slow_request_threshold.as_millis() as f64;

        // Determine if we need detailed logging that includes params and response.
        let needs_detailed_log = if let Some(code) = resp.as_error_code() {
            code == INTERNAL_ERROR_CODE || tracing::enabled!(tracing::Level::DEBUG)
        } else {
            elapsed_ms > slow_threshold_ms
        };

        // Only compute params and response when needed for detailed logging
        let (params, response) = if needs_detailed_log {
            let params = this.params.as_ref().map(|p| p.get()).unwrap_or("[]");
            let result = resp.as_result();
            let response = if result.len() > 1000 {
                format!("{}...", &result[..997])
            } else {
                result.to_string()
            };
            (params, response)
        } else {
            ("", String::new())
        };

        if let Some(code) = resp.as_error_code() {
            metrics
                .failed
                .with_label_values(&[method, &format!("{code}")])
                .inc();

            if code == INTERNAL_ERROR_CODE {
                error!(
                    method,
                    params, code, response, elapsed_ms, "Request failed with internal error"
                );
            } else if tracing::enabled!(tracing::Level::DEBUG) {
                debug!(
                    method,
                    params, code, response, elapsed_ms, "Request failed with non-internal error"
                );
            } else {
                info!(method, code, elapsed_ms, "Request failed");
            }
        } else {
            metrics.succeeded.with_label_values(&[method]).inc();
            if elapsed_ms > slow_threshold_ms {
                warn!(
                    method,
                    params,
                    response,
                    elapsed_ms,
                    threshold_ms = slow_threshold_ms,
                    "Slow request - exceeded threshold but succeeded"
                );
            } else {
                info!(method, elapsed_ms, "Request succeeded");
            }
        }

        Poll::Ready(resp)
    }
}

#[pinned_drop]
impl<F> PinnedDrop for MetricsFuture<'_, F> {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();

        if let Some(metrics) = this.metrics.take() {
            let method = this.method.as_ref();
            let elapsed_ms = metrics.timer.stop_and_record() * 1000.0;

            metrics.cancelled.with_label_values(&[method]).inc();

            info!(method, elapsed_ms, "Request cancelled");

            if elapsed_ms > this.slow_request_threshold.as_millis() as f64 {
                let params = this.params.as_ref().map(|p| p.get()).unwrap_or("[]");
                warn!(
                    method,
                    params, elapsed_ms, "Cancelled request took a long time"
                );
            }
        }
    }
}
