// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http;
use std::{borrow::Cow, sync::Arc, time::Instant};

use mysten_network::callback::{MakeCallbackHandler, ResponseHandler};
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, HistogramVec,
    IntCounterVec, IntGauge, IntGaugeVec, Registry,
};

#[derive(Clone)]
pub struct RpcMetrics {
    inflight_requests: IntGaugeVec,
    num_requests: IntCounterVec,
    request_latency: HistogramVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl RpcMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_requests: register_int_gauge_vec_with_registry!(
                "rpc_inflight_requests",
                "Total in-flight RPC requests per route",
                &["path"],
                registry,
            )
            .unwrap(),
            num_requests: register_int_counter_vec_with_registry!(
                "rpc_requests",
                "Total RPC requests per route and their http status",
                &["path", "status"],
                registry,
            )
            .unwrap(),
            request_latency: register_histogram_vec_with_registry!(
                "rpc_request_latency",
                "Latency of RPC requests per route",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone)]
pub struct RpcMetricsMakeCallbackHandler {
    metrics: Arc<RpcMetrics>,
}

impl RpcMetricsMakeCallbackHandler {
    pub fn new(metrics: Arc<RpcMetrics>) -> Self {
        Self { metrics }
    }
}

impl MakeCallbackHandler for RpcMetricsMakeCallbackHandler {
    type Handler = RpcMetricsCallbackHandler;

    fn make_handler(&self, request: &http::request::Parts) -> Self::Handler {
        let start = Instant::now();
        let metrics = self.metrics.clone();

        let path =
            if let Some(matched_path) = request.extensions.get::<axum::extract::MatchedPath>() {
                if request
                    .headers
                    .get(&http::header::CONTENT_TYPE)
                    .is_some_and(|header| header == tonic::metadata::GRPC_CONTENT_TYPE)
                {
                    Cow::Owned(request.uri.path().to_owned())
                } else {
                    Cow::Owned(matched_path.as_str().to_owned())
                }
            } else {
                Cow::Borrowed("unknown")
            };

        metrics
            .inflight_requests
            .with_label_values(&[path.as_ref()])
            .inc();

        RpcMetricsCallbackHandler {
            metrics,
            path,
            start,
            counted_response: false,
        }
    }
}

pub struct RpcMetricsCallbackHandler {
    metrics: Arc<RpcMetrics>,
    path: Cow<'static, str>,
    start: Instant,
    // Indicates if we successfully counted the response. In some cases when a request is
    // prematurely canceled this will remain false
    counted_response: bool,
}

impl ResponseHandler for RpcMetricsCallbackHandler {
    fn on_response(&mut self, response: &http::response::Parts) {
        const GRPC_STATUS: http::HeaderName = http::HeaderName::from_static("grpc-status");

        let status = if response
            .headers
            .get(&http::header::CONTENT_TYPE)
            .is_some_and(|content_type| {
                content_type
                    .as_bytes()
                    // check if the content-type starts_with 'application/grpc' in order to
                    // consider this as a gRPC request. A prefix comparison is done instead of a
                    // full equality check in order to account for the various types of
                    // content-types that are considered as gRPC traffic.
                    .starts_with(tonic::metadata::GRPC_CONTENT_TYPE.as_bytes())
            }) {
            let code = response
                .headers
                .get(&GRPC_STATUS)
                .map(http::HeaderValue::as_bytes)
                .map(tonic::Code::from_bytes)
                .unwrap_or(tonic::Code::Ok);

            code_as_str(code)
        } else {
            response.status.as_str()
        };

        self.metrics
            .num_requests
            .with_label_values(&[self.path.as_ref(), status])
            .inc();

        self.counted_response = true;
    }

    fn on_error<E>(&mut self, _error: &E) {
        // Do nothing if the whole service errored
        //
        // in Axum this isn't possible since all services are required to have an error type of
        // Infallible
    }
}

impl Drop for RpcMetricsCallbackHandler {
    fn drop(&mut self) {
        self.metrics
            .inflight_requests
            .with_label_values(&[self.path.as_ref()])
            .dec();

        let latency = self.start.elapsed().as_secs_f64();
        self.metrics
            .request_latency
            .with_label_values(&[self.path.as_ref()])
            .observe(latency);

        if !self.counted_response {
            self.metrics
                .num_requests
                .with_label_values(&[self.path.as_ref(), "canceled"])
                .inc();
        }
    }
}

fn code_as_str(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "canceled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid-argument",
        tonic::Code::DeadlineExceeded => "deadline-exceeded",
        tonic::Code::NotFound => "not-found",
        tonic::Code::AlreadyExists => "already-exists",
        tonic::Code::PermissionDenied => "permission-denied",
        tonic::Code::ResourceExhausted => "resource-exhausted",
        tonic::Code::FailedPrecondition => "failed-precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out-of-range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data-loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

#[derive(Clone)]
pub(crate) struct SubscriptionMetrics {
    pub inflight_subscribers: IntGauge,
    pub last_recieved_checkpoint: IntGauge,
}

impl SubscriptionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_subscribers: register_int_gauge_with_registry!(
                "subscription_inflight_subscribers",
                "Total in-flight subscriptions",
                registry,
            )
            .unwrap(),
            last_recieved_checkpoint: register_int_gauge_with_registry!(
                "subscription_last_recieved_checkpoint",
                "Last recieved checkpoint by the subscription service",
                registry,
            )
            .unwrap(),
        }
    }
}
