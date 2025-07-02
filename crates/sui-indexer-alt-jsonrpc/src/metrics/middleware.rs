// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Cow,
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use jsonrpsee::{
    server::middleware::rpc::RpcServiceT,
    types::{error::INTERNAL_ERROR_CODE, Request},
    MethodResponse,
};
use pin_project_lite::pin_project;
use prometheus::{HistogramTimer, IntCounterVec};
use serde_json::value::RawValue;
use tower_layer::Layer;
use tracing::{error, info};

use super::RpcMetrics;

/// Tower Layer that adds middleware to record statistics about RPC requests (how long they took to
/// serve, how many we have served, how many succeeded or failed, etc).
#[derive(Clone)]
pub(crate) struct MetricsLayer {
    metrics: Arc<RpcMetrics>,
    methods: Arc<HashSet<String>>,
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
}

pin_project! {
    pub(crate) struct MetricsFuture<'a, F> {
        metrics: Option<RequestMetrics>,
        method: Cow<'a, str>,
        // RPC request params for logging
        params: Option<Cow<'a, RawValue>>,
        #[pin]
        inner: F,
    }
}

impl MetricsLayer {
    /// Create a new metrics layer that only records statistics for the given methods (any other
    /// methods will be replaced with "<UNKNOWN>").
    pub fn new(metrics: Arc<RpcMetrics>, methods: HashSet<String>) -> Self {
        Self {
            metrics,
            methods: Arc::new(methods),
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
            }),
            method,
            params: request.params.clone(),
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

        if let Some(INTERNAL_ERROR_CODE) = resp.as_error_code() {
            metrics
                .failed
                .with_label_values(&[method, &format!("{INTERNAL_ERROR_CODE}")])
                .inc();

            let params = this.params.as_ref().map(|p| p.get()).unwrap_or("[]");
            let result = resp.as_result();
            let response = if result.len() > 1000 {
                format!("{}...", &result[..997])
            } else {
                result.to_string()
            };

            error!(
                method,
                params,
                code = INTERNAL_ERROR_CODE,
                response,
                elapsed_ms,
                "Internal error"
            );
        } else if let Some(code) = resp.as_error_code() {
            metrics
                .failed
                .with_label_values(&[method, &format!("{code}")])
                .inc();
            info!(method, code, elapsed_ms, "Request failed");
        } else {
            metrics.succeeded.with_label_values(&[method]).inc();
            info!(method, elapsed_ms, "Request succeeded");
        }

        Poll::Ready(resp)
    }
}
