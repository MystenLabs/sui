// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;
use tonic::codegen::http::header::HeaderName;
use tonic::codegen::http::{HeaderValue, Request, Response};
use tonic::{Code, Status};
use tower_http::classify::GrpcFailureClass;
use tower_http::trace::{OnFailure, OnRequest, OnResponse};
use tracing::Span;

pub(crate) static GRPC_ENDPOINT_PATH_HEADER: HeaderName = HeaderName::from_static("grpc-path-req");

/// The trait to be implemented when want to be notified about
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
