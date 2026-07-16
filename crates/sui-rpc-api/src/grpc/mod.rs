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

    pub fn into_router(
        self,
        request_log: mysten_network::request_log::GrpcRequestLogLayer,
    ) -> axum::Router {
        let timeout = self.timeout;
        self.router
            // The capture layer sits under `GrpcWebLayer` (the last layer added is outermost) so
            // it always sees standard gRPC frames, including for grpc-web(-text) requests.
            .layer(request_log)
            // The timeout sits inside the grpc-web layer so that its
            // trailers-only DeadlineExceeded response is translated for
            // grpc-web clients too.
            .layer(layer_fn(move |service| GrpcTimeout::new(service, timeout)))
            .layer(tonic_web::GrpcWebLayer::new())
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::task::Context;
    use std::task::Poll;

    use base64::Engine as _;
    use mysten_network::request_log::GrpcRequestLogLayer;
    use prost::Message;
    use tower::ServiceExt;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

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

    /// A request-log layer with an empty descriptor pool: `capture_state` never resolves a
    /// service/method against it, so it's a pure pass-through — for tests that exercise
    /// unrelated `Services` behavior and don't care about capture.
    fn empty_request_log() -> GrpcRequestLogLayer {
        GrpcRequestLogLayer::from_encoded_file_descriptor_sets([]).unwrap()
    }

    /// The server-side default deadline must bound a request whose handler
    /// never completes, surfacing gRPC status 4 (DeadlineExceeded) instead
    /// of hanging the client forever.
    #[tokio::test(start_paused = true)]
    async fn server_timeout_bounds_a_hung_handler() {
        let router = Services::new()
            .timeout(Some(Duration::from_millis(50)))
            .add_service(HangingService)
            .into_router(empty_request_log());

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
            .into_router(empty_request_log());

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
            .into_router(empty_request_log());

        let response = tokio::time::timeout(
            Duration::from_secs(24 * 60 * 60),
            router.oneshot(request(None)),
        )
        .await;
        assert!(response.is_err(), "request completed without a deadline");
    }

    /// Records the `payload` field of every `grpc_request` event.
    #[derive(Clone, Default)]
    struct CaptureLayer {
        payloads: Arc<Mutex<Vec<String>>>,
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            struct Visitor(Option<String>);
            impl tracing::field::Visit for Visitor {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "payload" {
                        self.0 = Some(format!("{value:?}"));
                    }
                }
            }

            let mut visitor = Visitor(None);
            event.record(&mut visitor);
            if let Some(payload) = visitor.0 {
                self.payloads.lock().unwrap().push(payload);
            }
        }
    }

    /// The request-log layer must sit *under* `GrpcWebLayer` so it sees standard gRPC frames for
    /// grpc-web requests too. If the ordering regresses, the capture stream silently loses all
    /// browser-client traffic.
    #[tokio::test]
    async fn request_log_captures_grpc_web_requests() {
        let capture_layer = CaptureLayer::default();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("grpc_request=trace"))
            .with(capture_layer.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        let (_health_reporter, health_service) = tonic_health::server::health_reporter();
        let router = Services::new().add_service(health_service).into_router(
            GrpcRequestLogLayer::from_encoded_file_descriptor_sets([
                tonic_health::pb::FILE_DESCRIPTOR_SET,
            ])
            .unwrap(),
        );

        let message = tonic_health::pb::HealthCheckRequest {
            service: "x".to_owned(),
        }
        .encode_to_vec();
        let mut body = vec![0u8];
        body.extend_from_slice(&(message.len() as u32).to_be_bytes());
        body.extend_from_slice(&message);

        let request = axum::http::Request::builder()
            .method(axum::http::Method::POST)
            .uri("/grpc.health.v1.Health/Check")
            .header(
                axum::http::header::CONTENT_TYPE,
                "application/grpc-web+proto",
            )
            .body(axum::body::Body::from(body))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let payloads = capture_layer.payloads.lock().unwrap();
        assert_eq!(
            *payloads,
            vec![base64::engine::general_purpose::STANDARD.encode(&message)]
        );
    }
}
