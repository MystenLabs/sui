// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use hyper::{header::HeaderValue, Request, Response};
use std::task::{Context, Poll};
use tower::Layer;
use tower::Service;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TraceIdLayer;

impl<S> Layer<S> for TraceIdLayer {
    type Service = TraceIdMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TraceIdMiddleware::new(inner)
    }
}

#[derive(Debug, Clone)]
pub struct TraceIdMiddleware<S> {
    inner: S,
}

impl<S> TraceIdMiddleware<S> {
    pub fn new(inner: S) -> Self {
        TraceIdMiddleware { inner }
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for TraceIdMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let trace_id = Uuid::new_v4();
        req.headers_mut().insert(
            "x-trace-id",
            HeaderValue::from_str(&trace_id.to_string()).unwrap(),
        );

        self.inner.call(req)
    }
}
