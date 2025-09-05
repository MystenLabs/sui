// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::Cow, sync::Arc};

use axum::extract::MatchedPath;
use http::{header::CONTENT_TYPE, StatusCode};
use mysten_network::callback::{MakeCallbackHandler, ResponseHandler};
use prometheus::HistogramTimer;
use tonic::{metadata::GRPC_CONTENT_TYPE, Code};

use crate::rpc::metrics::RpcMetrics;

#[derive(Clone)]
pub struct MakeMetricsHandler {
    metrics: Arc<RpcMetrics>,
}

pub struct Handler {
    metrics: Arc<RpcMetrics>,
    path: Cow<'static, str>,
    timer: Option<HistogramTimer>,
}

impl MakeMetricsHandler {
    pub fn new(metrics: Arc<RpcMetrics>) -> Self {
        Self { metrics }
    }
}

impl MakeCallbackHandler for MakeMetricsHandler {
    type Handler = Handler;

    fn make_handler(&self, request: &http::request::Parts) -> Self::Handler {
        let metrics = self.metrics.clone();

        let path = if let Some(matched_path) = request.extensions.get::<MatchedPath>() {
            if request
                .headers
                .get(&CONTENT_TYPE)
                .is_some_and(|header| header == GRPC_CONTENT_TYPE)
            {
                Cow::Owned(request.uri.path().to_owned())
            } else {
                Cow::Owned(matched_path.as_str().to_owned())
            }
        } else {
            Cow::Borrowed("unknown")
        };

        let timer = metrics
            .request_latency
            .with_label_values(&[path.as_ref()])
            .start_timer();

        metrics
            .requests_received
            .with_label_values(&[path.as_ref()])
            .inc();

        Handler {
            metrics,
            path,
            timer: Some(timer),
        }
    }
}

impl ResponseHandler for Handler {
    fn on_response(&mut self, response: &http::response::Parts) {
        const GRPC_STATUS: http::HeaderName = http::HeaderName::from_static("grpc-status");

        let (success, status) = if response
            .headers
            .get(&CONTENT_TYPE)
            .is_some_and(|content_type| {
                content_type
                    .as_bytes()
                    // check if the content-type starts_with 'application/grpc' in order to
                    // consider this as a gRPC request. A prefix comparison is done instead of a
                    // full equality check in order to account for the various types of
                    // content-types that are considered as gRPC traffic.
                    .starts_with(GRPC_CONTENT_TYPE.as_bytes())
            }) {
            let code = response
                .headers
                .get(&GRPC_STATUS)
                .map(http::HeaderValue::as_bytes)
                .map(Code::from_bytes)
                .unwrap_or(Code::Ok);

            (code == Code::Ok, code_as_str(code))
        } else {
            (response.status == StatusCode::OK, response.status.as_str())
        };

        self.timer.take().map(HistogramTimer::stop_and_record);

        if success {
            self.metrics
                .requests_succeeded
                .with_label_values(&[self.path.as_ref(), status])
                .inc();
        } else {
            self.metrics
                .requests_failed
                .with_label_values(&[self.path.as_ref(), status])
                .inc();
        }
    }

    fn on_error<E>(&mut self, _error: &E) {
        unreachable!("all axum services are required to have an error type of Infallible");
    }
}

impl Drop for Handler {
    fn drop(&mut self) {
        if self.timer.is_some() {
            self.metrics
                .requests_cancelled
                .with_label_values(&[self.path.as_ref()])
                .inc();
        }
    }
}

fn code_as_str(code: Code) -> &'static str {
    match code {
        Code::Ok => "ok",
        Code::Cancelled => "cancelled",
        Code::Unknown => "unknown",
        Code::InvalidArgument => "invalid-argument",
        Code::DeadlineExceeded => "deadline-exceeded",
        Code::NotFound => "not-found",
        Code::AlreadyExists => "already-exists",
        Code::PermissionDenied => "permission-denied",
        Code::ResourceExhausted => "resource-exhausted",
        Code::FailedPrecondition => "failed-precondition",
        Code::Aborted => "aborted",
        Code::OutOfRange => "out-of-range",
        Code::Unimplemented => "unimplemented",
        Code::Internal => "internal",
        Code::Unavailable => "unavailable",
        Code::DataLoss => "data-loss",
        Code::Unauthenticated => "unauthenticated",
    }
}
