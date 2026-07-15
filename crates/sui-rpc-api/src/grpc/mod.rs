// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;
use std::time::Duration;
use sui_http::middleware::grpc_timeout::GrpcTimeout;
use tonic::server::NamedService;
use tower::Service;
use tower::layer::layer_fn;

pub mod deadline;
pub mod v2;
pub mod v2alpha;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Default)]
pub struct Services {
    router: axum::Router,
    timeout: Option<Duration>,
}

impl Services {
    pub fn new() -> Self {
        Self::default()
    }

    /// Server-side deadline applied to every gRPC request mounted here.
    /// Requests carrying a `grpc-timeout` header are bounded by the smaller
    /// of the two values. The deadline covers a unary request's execution
    /// and a streaming request's time to first response; it does not bound
    /// the lifetime of an established stream.
    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Add a new service.
    pub fn add_service<S>(mut self, svc: S) -> Self
    where
        S: Service<
                axum::extract::Request,
                Response: axum::response::IntoResponse,
                Error = Infallible,
            > + NamedService
            + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
        S::Error: Into<BoxError> + Send,
    {
        self.router = self
            .router
            .route_service(&format!("/{}/{{*rest}}", S::NAME), svc);
        self
    }

    pub fn merge_router(mut self, router: axum::Router) -> Self {
        self.router = self.router.merge(router);
        self
    }

    pub fn into_router(self) -> axum::Router {
        let timeout = self.timeout;
        self.router
            // The timeout sits inside the grpc-web layer so that its
            // trailers-only DeadlineExceeded response is translated for
            // grpc-web clients too.
            .layer(layer_fn(move |service| GrpcTimeout::new(service, timeout)))
            .layer(tonic_web::GrpcWebLayer::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;
    use tower::ServiceExt;

    /// A gRPC service whose handler never completes, standing in for a
    /// request wedged on a lock, a stalled backend, or an h2 send window
    /// that never reopens.
    #[derive(Clone)]
    struct HangingService;

    impl Service<axum::extract::Request> for HangingService {
        type Response = axum::response::Response;
        type Error = Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: axum::extract::Request) -> Self::Future {
            Box::pin(std::future::pending())
        }
    }

    impl NamedService for HangingService {
        const NAME: &'static str = "test.Hanging";
    }

    fn request(grpc_timeout: Option<&str>) -> axum::extract::Request {
        let mut builder = http::Request::builder()
            .method(http::Method::POST)
            .uri("/test.Hanging/Method")
            // The grpc-web layer only passes through native gRPC requests
            // arriving over HTTP/2.
            .version(http::Version::HTTP_2)
            .header(http::header::CONTENT_TYPE, "application/grpc");
        if let Some(timeout) = grpc_timeout {
            builder = builder.header("grpc-timeout", timeout);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    fn grpc_status(response: &http::Response<axum::body::Body>) -> Option<&str> {
        response
            .headers()
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
    }

    /// The server-side default deadline must bound a request whose handler
    /// never completes, surfacing gRPC status 4 (DeadlineExceeded) instead
    /// of hanging the client forever.
    #[tokio::test(start_paused = true)]
    async fn server_timeout_bounds_a_hung_handler() {
        let router = Services::new()
            .timeout(Some(Duration::from_millis(50)))
            .add_service(HangingService)
            .into_router();

        let response = router.oneshot(request(None)).await.unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(grpc_status(&response), Some("4"));
    }

    /// A client-supplied `grpc-timeout` header must be honored even when no
    /// server default is configured.
    #[tokio::test(start_paused = true)]
    async fn client_grpc_timeout_header_is_honored() {
        let router = Services::new()
            .timeout(None)
            .add_service(HangingService)
            .into_router();

        let response = router.oneshot(request(Some("50m"))).await.unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(grpc_status(&response), Some("4"));
    }

    /// With no server default and no client header there is no deadline:
    /// the request must still be pending well past any implicit bound. This
    /// pins the disabled behavior (config value `0`).
    #[tokio::test(start_paused = true)]
    async fn no_timeout_means_no_deadline() {
        let router = Services::new()
            .timeout(None)
            .add_service(HangingService)
            .into_router();

        let response = tokio::time::timeout(
            Duration::from_secs(24 * 60 * 60),
            router.oneshot(request(None)),
        )
        .await;
        assert!(response.is_err(), "request completed without a deadline");
    }
}
