// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use axum::body::Body;
use futures::FutureExt;
use http::Request;
use http::Response;
use pin_project::pin_project;
use tonic::Status;
use tower::Layer;
use tower::Service;

use crate::rpc::metrics::RpcMetrics;

/// Tower layer that catches panics during request processing and converts them to gRPC
/// `Status::internal()` errors.
#[derive(Clone)]
pub struct CatchPanicLayer {
    metrics: Arc<RpcMetrics>,
}

/// Tower service that catches panics and returns gRPC internal errors.
#[derive(Clone)]
pub struct CatchPanicService<S> {
    inner: S,
    metrics: Arc<RpcMetrics>,
}

/// Future that catches panics and converts them to gRPC errors.
#[pin_project]
pub struct CatchPanicFuture<F> {
    #[pin]
    inner: futures::future::CatchUnwind<AssertUnwindSafe<F>>,
    metrics: Arc<RpcMetrics>,
}

impl CatchPanicLayer {
    pub fn new(metrics: Arc<RpcMetrics>) -> Self {
        Self { metrics }
    }
}

impl<S> Layer<S> for CatchPanicLayer {
    type Service = CatchPanicService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CatchPanicService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for CatchPanicService<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = CatchPanicFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        CatchPanicFuture {
            inner: AssertUnwindSafe(self.inner.call(req)).catch_unwind(),
            metrics: self.metrics.clone(),
        }
    }
}

impl<F, E> Future for CatchPanicFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(err)) => {
                this.metrics.requests_panicked.inc();

                let status = Status::internal(if let Some(s) = err.downcast_ref::<String>() {
                    format!("Request panicked: {s}")
                } else if let Some(s) = err.downcast_ref::<&str>() {
                    format!("Request panicked: {s}")
                } else {
                    "Request panicked".to_string()
                });

                Poll::Ready(Ok(status.into_http()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use prometheus::Registry;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn test_catch_panic() {
        let registry = Registry::new();
        let metrics = Arc::new(RpcMetrics::new(&registry));

        #[allow(unreachable_code)]
        let panic_service = tower::service_fn(|_: Request<Body>| async {
            panic!("Boom!");
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let service = CatchPanicLayer::new(metrics.clone()).layer(panic_service);
        let request = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let response = service.oneshot(request).await.unwrap();

        let status = response
            .headers()
            .get("grpc-status")
            .expect("Should have grpc-status header");

        assert_eq!(status, &(tonic::Code::Internal as usize).to_string());
        assert_eq!(metrics.requests_panicked.get(), 1);
    }

    #[tokio::test]
    async fn test_catch_panic_passthrough() {
        let registry = Registry::new();
        let metrics = Arc::new(RpcMetrics::new(&registry));

        let ok_service = tower::service_fn(|_req: Request<Body>| async {
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let service = CatchPanicLayer::new(metrics.clone()).layer(ok_service);
        let request = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let response = service.oneshot(request).await.unwrap();

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(metrics.requests_panicked.get(), 0);
    }
}
