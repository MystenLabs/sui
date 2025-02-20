// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use futures::FutureExt;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramVec, IntCounterVec, IntGaugeVec,
};
use sui_json_rpc_api::TRANSIENT_ERROR_CODE;
use sui_json_rpc_api::{CLIENT_SDK_TYPE_HEADER, CLIENT_TARGET_API_VERSION_HEADER};
use tokio::time::Instant;

const SPAM_LABEL: &str = "SPAM";
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

#[derive(Debug, Clone)]
pub struct Metrics {
    /// Counter of requests, route is a label (ie separate timeseries per route)
    requests_by_route: IntCounterVec,
    /// Gauge of inflight requests, route is a label (ie separate timeseries per route)
    inflight_requests_by_route: IntGaugeVec,
    /// Request latency, route is a label
    req_latency_by_route: HistogramVec,
    /// Failed requests by route
    errors_by_route: IntCounterVec,
    server_errors_by_route: IntCounterVec,
    client_errors_by_route: IntCounterVec,
    transient_errors_by_route: IntCounterVec,
    /// Client info
    client: IntCounterVec,
    /// Request size
    rpc_request_size: HistogramVec,
    /// Response size
    rpc_response_size: HistogramVec,

    method_whitelist: HashSet<String>,
}

impl Metrics {
    pub fn new(registry: &prometheus::Registry, method_whitelist: &[&str]) -> Self {
        Self {
            requests_by_route: register_int_counter_vec_with_registry!(
                "rpc_requests_by_route",
                "Number of requests by route",
                &["route"],
                registry,
            )
            .unwrap(),
            inflight_requests_by_route: register_int_gauge_vec_with_registry!(
                "inflight_rpc_requests_by_route",
                "Number of inflight requests by route",
                &["route"],
                registry,
            )
            .unwrap(),
            req_latency_by_route: register_histogram_vec_with_registry!(
                "req_latency_by_route",
                "Latency of a request by route",
                &["route"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            client_errors_by_route: register_int_counter_vec_with_registry!(
                "client_errors_by_route",
                "Number of client errors by route",
                &["route"],
                registry,
            )
            .unwrap(),
            server_errors_by_route: register_int_counter_vec_with_registry!(
                "server_errors_by_route",
                "Number of server errors by route",
                &["route"],
                registry,
            )
            .unwrap(),
            transient_errors_by_route: register_int_counter_vec_with_registry!(
                "transient_errors_by_route",
                "Number of transient errors by route",
                &["route"],
                registry,
            )
            .unwrap(),
            errors_by_route: register_int_counter_vec_with_registry!(
                "errors_by_route",
                "Number of client and server errors by route",
                &["route"],
                registry
            )
            .unwrap(),
            client: register_int_counter_vec_with_registry!(
                "rpc_client",
                "Connected RPC client's info",
                &["client_type", "api_version"],
                registry,
            )
            .unwrap(),
            rpc_request_size: register_histogram_vec_with_registry!(
                "rpc_request_size",
                "Request size of rpc requests",
                &["route"],
                prometheus::exponential_buckets(32.0, 2.0, 19)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            rpc_response_size: register_histogram_vec_with_registry!(
                "rpc_response_size",
                "Response size of rpc requests",
                &["route"],
                prometheus::exponential_buckets(1024.0, 2.0, 20)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            method_whitelist: method_whitelist.iter().map(|s| (*s).into()).collect(),
        }
    }

    fn check_spam<'a>(&'a self, method_name: &'a str) -> &'a str {
        if self.method_whitelist.contains(method_name) {
            method_name
        } else {
            SPAM_LABEL
        }
    }

    fn on_request(&self, request: &jsonrpsee::types::Request<'_>) {
        let method_name = request.method_name();
        let method_name = self.check_spam(method_name);
        self.inflight_requests_by_route
            .with_label_values(&[method_name])
            .inc();
        self.requests_by_route
            .with_label_values(&[method_name])
            .inc();

        self.rpc_request_size
            .with_label_values(&[method_name])
            .observe(request.params().len_bytes() as f64);
    }

    fn on_response(
        &self,
        method_name: &str,
        started_at: Instant,
        response: &jsonrpsee::MethodResponse,
    ) {
        let method_name = self.check_spam(method_name);
        self.inflight_requests_by_route
            .with_label_values(&[method_name])
            .dec();
        let req_latency_secs = (Instant::now() - started_at).as_secs_f64();
        self.req_latency_by_route
            .with_label_values(&[method_name])
            .observe(req_latency_secs);

        if let Some(code) = response.as_error_code() {
            if code == jsonrpsee::types::error::CALL_EXECUTION_FAILED_CODE
                || code == jsonrpsee::types::error::INTERNAL_ERROR_CODE
            {
                self.server_errors_by_route
                    .with_label_values(&[method_name])
                    .inc();
            } else if code == TRANSIENT_ERROR_CODE {
                self.transient_errors_by_route
                    .with_label_values(&[method_name])
                    .inc();
            } else {
                self.client_errors_by_route
                    .with_label_values(&[method_name])
                    .inc();
            }
            self.errors_by_route.with_label_values(&[method_name]).inc();
        }

        self.rpc_response_size
            .with_label_values(&[method_name])
            .observe(response.as_result().len() as f64)
    }

    pub fn on_http_request(&self, headers: &axum::http::HeaderMap<axum::http::HeaderValue>) {
        let client_type = headers
            .get(CLIENT_SDK_TYPE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown");

        let api_version = headers
            .get(CLIENT_TARGET_API_VERSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown");
        self.client
            .with_label_values(&[client_type, api_version])
            .inc();
    }
}

#[derive(Clone)]
pub struct MetricsLayer<S> {
    inner: S,
    metrics: Arc<Metrics>,
}

impl<S> MetricsLayer<S> {
    pub fn new(service: S, metrics: Arc<Metrics>) -> Self {
        Self {
            inner: service,
            metrics,
        }
    }
}

impl<'a, S> RpcServiceT<'a> for MetricsLayer<S>
where
    S: RpcServiceT<'a> + Send + Sync,
    S::Future: 'a,
{
    type Future = futures::future::BoxFuture<'a, jsonrpsee::MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let metrics = self.metrics.clone();
        metrics.on_request(&req);
        let method_name = req.method_name().to_owned();
        let started_at = Instant::now();
        let fut = self.inner.call(req);

        async move {
            let response = fut.await;
            metrics.on_response(&method_name, started_at, &response);
            response
        }
        .boxed()
    }
}
