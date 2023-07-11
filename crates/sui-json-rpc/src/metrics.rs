// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use hyper::body::HttpBody;
use std::collections::HashSet;
use std::net::SocketAddr;

use crate::{CLIENT_SDK_TYPE_HEADER, CLIENT_TARGET_API_VERSION_HEADER};
use jsonrpsee::server::logger::{HttpRequest, Logger, MethodKind, TransportProtocol};
use jsonrpsee::types::Params;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramVec, IntCounterVec, IntGaugeVec,
};
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
    /// Client info
    client: IntCounterVec,
    /// Connection count
    inflight_connection: IntGaugeVec,
    /// Request size
    rpc_request_size: HistogramVec,
    /// Response size
    rpc_response_size: HistogramVec,
}

#[derive(Clone)]
pub struct MetricsLogger {
    metrics: Metrics,
    method_whitelist: HashSet<String>,
}

impl MetricsLogger {
    fn check_spam<'a>(&'a self, method_name: &'a str) -> &'a str {
        if self.method_whitelist.contains(method_name) {
            method_name
        } else {
            SPAM_LABEL
        }
    }

    pub fn new(registry: &prometheus::Registry, method_whitelist: &[&str]) -> Self {
        let metrics = Metrics {
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
            inflight_connection: register_int_gauge_vec_with_registry!(
                "rpc_inflight_connection",
                "Number of inflight RPC connection by protocol",
                &["protocol"],
                registry,
            )
            .unwrap(),
            rpc_request_size: register_histogram_vec_with_registry!(
                "rpc_request_size",
                "Request size of rpc requests",
                &["protocol"],
                prometheus::exponential_buckets(32.0, 2.0, 19)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            rpc_response_size: register_histogram_vec_with_registry!(
                "rpc_response_size",
                "Response size of rpc requests",
                &["protocol"],
                prometheus::exponential_buckets(1024.0, 2.0, 20)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
        };

        Self {
            metrics,
            method_whitelist: method_whitelist.iter().map(|s| (*s).into()).collect(),
        }
    }
}

impl Logger for MetricsLogger {
    type Instant = Instant;

    fn on_connect(&self, _remote_addr: SocketAddr, request: &HttpRequest, t: TransportProtocol) {
        let client_type = request
            .headers()
            .get(CLIENT_SDK_TYPE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown");

        let api_version = request
            .headers()
            .get(CLIENT_TARGET_API_VERSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown");
        self.metrics
            .client
            .with_label_values(&[client_type, api_version])
            .inc();
        self.metrics
            .inflight_connection
            .with_label_values(&[&t.to_string()])
            .inc();

        self.metrics
            .rpc_request_size
            .with_label_values(&[&t.to_string()])
            .observe(
                request
                    .size_hint()
                    .exact()
                    .unwrap_or_else(|| request.size_hint().lower()) as f64,
            );
    }

    fn on_request(&self, _transport: TransportProtocol) -> Self::Instant {
        Instant::now()
    }

    fn on_call(
        &self,
        method_name: &str,
        _params: Params,
        _kind: MethodKind,
        _transport: TransportProtocol,
    ) {
        let method_name = self.check_spam(method_name);
        self.metrics
            .inflight_requests_by_route
            .with_label_values(&[method_name])
            .inc();
        self.metrics
            .requests_by_route
            .with_label_values(&[method_name])
            .inc();
    }

    fn on_result(
        &self,
        method_name: &str,
        _success: bool,
        error_code: Option<i32>,
        started_at: Self::Instant,
        _transport: TransportProtocol,
    ) {
        let method_name = self.check_spam(method_name);
        self.metrics
            .inflight_requests_by_route
            .with_label_values(&[method_name])
            .dec();
        let req_latency_secs = (Instant::now() - started_at).as_secs_f64();
        self.metrics
            .req_latency_by_route
            .with_label_values(&[method_name])
            .observe(req_latency_secs);

        if let Some(code) = error_code {
            if code == -32000 {
                self.metrics
                    .server_errors_by_route
                    .with_label_values(&[method_name])
                    .inc();
            } else {
                self.metrics
                    .client_errors_by_route
                    .with_label_values(&[method_name])
                    .inc();
            }
            self.metrics
                .errors_by_route
                .with_label_values(&[method_name])
                .inc();
        }
    }

    fn on_response(&self, result: &str, _started_at: Self::Instant, t: TransportProtocol) {
        self.metrics
            .rpc_response_size
            .with_label_values(&[&t.to_string()])
            .observe(std::mem::size_of_val(result) as f64)
    }

    fn on_disconnect(&self, _remote_addr: SocketAddr, t: TransportProtocol) {
        self.metrics
            .inflight_connection
            .with_label_values(&[&t.to_string()])
            .dec();
    }
}
