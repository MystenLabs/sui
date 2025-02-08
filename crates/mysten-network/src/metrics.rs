// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use anemo_tower::callback::{MakeCallbackHandler, ResponseHandler};
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramTimer, HistogramVec, IntCounterVec, IntGaugeVec,
    Registry,
};
use tonic::codegen::http::header::HeaderName;
use tonic::codegen::http::{HeaderValue, Request, Response};
use tonic::{Code, Status};
use tower_http::classify::GrpcFailureClass;
use tower_http::trace::{OnFailure, OnRequest, OnResponse};
use tracing::{warn, Span};

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

// Arbitrarily chosen buckets for message size, with gradually-lowering exponent to give us
// better resolution at high sizes.
const SIZE_BYTE_BUCKETS: &[f64] = &[
    2048., 8192., // *4
    16384., 32768., 65536., 131072., 262144., 524288., 1048576., // *2
    1572864., 2359256., 3538944., // *1.5
    4600627., 5980815., 7775060., 10107578., 13139851., 17081807., 22206349., 28868253., 37528729.,
    48787348., 63423553., // *1.3
];

pub(crate) static GRPC_ENDPOINT_PATH_HEADER: HeaderName = HeaderName::from_static("grpc-path-req");

/// The trait to be implemented when you want to be notified about
/// a new request and related metrics around it. When a request
/// is performed (up to the point that a response is created) the
/// on_response method is called with the corresponding metrics
/// details. The on_request method will be called when the request
/// is received, but not further processing has happened at this
/// point.
pub trait MetricsCallbackProvider: Send + Sync + Clone + 'static {
    /// Method will be called when a request has been received.
    /// `path`: the endpoint uri path
    fn on_request(&self, path: String);

    /// Method to be called from the server when a request is performed.
    /// `path`: the endpoint uri path
    /// `latency`: the time when the request was received and when the response was created
    /// `status`: the http status code of the response
    /// `grpc_status_code`: the grpc status code (see <https://github.com/grpc/grpc/blob/master/doc/statuscodes.md#status-codes-and-their-use-in-grpc>)
    fn on_response(&self, path: String, latency: Duration, status: u16, grpc_status_code: Code);

    /// Called when request call is started
    fn on_start(&self, _path: &str) {}

    /// Called when request call is dropped.
    /// It is guaranteed that for each on_start there will be corresponding on_drop
    fn on_drop(&self, _path: &str) {}
}

#[derive(Clone, Default)]
pub struct DefaultMetricsCallbackProvider {}
impl MetricsCallbackProvider for DefaultMetricsCallbackProvider {
    fn on_request(&self, _path: String) {}

    fn on_response(
        &self,
        _path: String,
        _latency: Duration,
        _status: u16,
        _grpc_status_code: Code,
    ) {
    }
}

#[derive(Clone)]
pub(crate) struct MetricsHandler<M: MetricsCallbackProvider> {
    metrics_provider: M,
}

impl<M: MetricsCallbackProvider> MetricsHandler<M> {
    pub(crate) fn new(metrics_provider: M) -> Self {
        Self { metrics_provider }
    }
}

impl<B, M: MetricsCallbackProvider> OnResponse<B> for MetricsHandler<M> {
    fn on_response(self, response: &Response<B>, latency: Duration, _span: &Span) {
        let grpc_status = Status::from_header_map(response.headers());
        let grpc_status_code = grpc_status.map_or(Code::Ok, |s| s.code());

        let path: HeaderValue = response
            .headers()
            .get(&GRPC_ENDPOINT_PATH_HEADER)
            .unwrap()
            .clone();

        self.metrics_provider.on_response(
            path.to_str().unwrap().to_string(),
            latency,
            response.status().as_u16(),
            grpc_status_code,
        );
    }
}

impl<B, M: MetricsCallbackProvider> OnRequest<B> for MetricsHandler<M> {
    fn on_request(&mut self, request: &Request<B>, _span: &Span) {
        self.metrics_provider
            .on_request(request.uri().path().to_string());
    }
}

impl<M: MetricsCallbackProvider> OnFailure<GrpcFailureClass> for MetricsHandler<M> {
    fn on_failure(
        &mut self,
        _failure_classification: GrpcFailureClass,
        _latency: Duration,
        _span: &Span,
    ) {
        // just do nothing for now so we avoid printing unnecessary logs
    }
}

#[derive(Clone)]
pub struct NetworkMetrics {
    /// Counter of requests by route
    requests: IntCounterVec,
    /// Request latency by route
    request_latency: HistogramVec,
    /// Request size by route
    request_size: HistogramVec,
    /// Response size by route
    response_size: HistogramVec,
    /// Counter of requests exceeding the "excessive" size limit
    excessive_size_requests: IntCounterVec,
    /// Counter of responses exceeding the "excessive" size limit
    excessive_size_responses: IntCounterVec,
    /// Gauge of the number of inflight requests at any given time by route
    inflight_requests: IntGaugeVec,
    /// Failed requests by route
    errors: IntCounterVec,
}

impl NetworkMetrics {
    pub fn new(node: &'static str, direction: &'static str, registry: &Registry) -> Self {
        let requests = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_requests"),
            "The number of requests made on the network",
            &["route"],
            registry
        )
        .unwrap();

        let request_latency = register_histogram_vec_with_registry!(
            format!("{node}_{direction}_request_latency"),
            "Latency of a request by route",
            &["route"],
            LATENCY_SEC_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let request_size = register_histogram_vec_with_registry!(
            format!("{node}_{direction}_request_size"),
            "Size of a request by route",
            &["route"],
            SIZE_BYTE_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let response_size = register_histogram_vec_with_registry!(
            format!("{node}_{direction}_response_size"),
            "Size of a response by route",
            &["route"],
            SIZE_BYTE_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let excessive_size_requests = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_excessive_size_requests"),
            "The number of excessively large request messages sent",
            &["route"],
            registry
        )
        .unwrap();

        let excessive_size_responses = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_excessive_size_responses"),
            "The number of excessively large response messages seen",
            &["route"],
            registry
        )
        .unwrap();

        let inflight_requests = register_int_gauge_vec_with_registry!(
            format!("{node}_{direction}_inflight_requests"),
            "The number of inflight network requests",
            &["route"],
            registry
        )
        .unwrap();

        let errors = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_request_errors"),
            "Number of errors by route",
            &["route", "status"],
            registry,
        )
        .unwrap();

        Self {
            requests,
            request_latency,
            request_size,
            response_size,
            excessive_size_requests,
            excessive_size_responses,
            inflight_requests,
            errors,
        }
    }
}

#[derive(Clone)]
pub struct MetricsMakeCallbackHandler {
    metrics: Arc<NetworkMetrics>,
    /// Size in bytes above which a request or response message is considered excessively large
    excessive_message_size: usize,
}

impl MetricsMakeCallbackHandler {
    pub fn new(metrics: Arc<NetworkMetrics>, excessive_message_size: usize) -> Self {
        Self {
            metrics,
            excessive_message_size,
        }
    }
}

impl MakeCallbackHandler for MetricsMakeCallbackHandler {
    type Handler = MetricsResponseHandler;

    fn make_handler(&self, request: &anemo::Request<bytes::Bytes>) -> Self::Handler {
        let route = request.route().to_owned();

        self.metrics.requests.with_label_values(&[&route]).inc();
        self.metrics
            .inflight_requests
            .with_label_values(&[&route])
            .inc();
        let body_len = request.body().len();
        self.metrics
            .request_size
            .with_label_values(&[&route])
            .observe(body_len as f64);
        if body_len > self.excessive_message_size {
            warn!(
                "Saw excessively large request with size {body_len} for {route} with peer {:?}",
                request.peer_id()
            );
            self.metrics
                .excessive_size_requests
                .with_label_values(&[&route])
                .inc();
        }

        let timer = self
            .metrics
            .request_latency
            .with_label_values(&[&route])
            .start_timer();

        MetricsResponseHandler {
            metrics: self.metrics.clone(),
            timer,
            route,
            excessive_message_size: self.excessive_message_size,
        }
    }
}

pub struct MetricsResponseHandler {
    metrics: Arc<NetworkMetrics>,
    // The timer is held on to and "observed" once dropped
    #[allow(unused)]
    timer: HistogramTimer,
    route: String,
    excessive_message_size: usize,
}

impl ResponseHandler for MetricsResponseHandler {
    fn on_response(self, response: &anemo::Response<bytes::Bytes>) {
        let body_len = response.body().len();
        self.metrics
            .response_size
            .with_label_values(&[&self.route])
            .observe(body_len as f64);
        if body_len > self.excessive_message_size {
            warn!(
                "Saw excessively large response with size {body_len} for {} with peer {:?}",
                self.route,
                response.peer_id()
            );
            self.metrics
                .excessive_size_responses
                .with_label_values(&[&self.route])
                .inc();
        }

        if !response.status().is_success() {
            let status = response.status().to_u16().to_string();
            self.metrics
                .errors
                .with_label_values(&[&self.route, &status])
                .inc();
        }
    }

    fn on_error<E>(self, _error: &E) {
        self.metrics
            .errors
            .with_label_values(&[&self.route, "unknown"])
            .inc();
    }
}

impl Drop for MetricsResponseHandler {
    fn drop(&mut self) {
        self.metrics
            .inflight_requests
            .with_label_values(&[&self.route])
            .dec();
    }
}
