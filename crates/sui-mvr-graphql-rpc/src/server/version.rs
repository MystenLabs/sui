// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    body::Body,
    extract::State,
    http::{HeaderName, HeaderValue, Request},
    middleware::Next,
    response::Response,
};

use crate::config::Version;

pub(crate) static VERSION_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-version");

/// Mark every outgoing response with a header indicating the precise version of the RPC that was
/// used (including the patch version and sha).
pub(crate) async fn set_version_middleware(
    State(version): State<Version>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        VERSION_HEADER.clone(),
        HeaderValue::from_static(version.full),
    );
    response
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;
    use crate::{
        config::{ConnectionConfig, ServiceConfig, Version},
        metrics::Metrics,
        server::builder::AppState,
    };
    use axum::{body::Body, middleware, routing::get, Router};
    use http::StatusCode;
    use mysten_metrics;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    fn metrics() -> Metrics {
        let binding_address: SocketAddr = "0.0.0.0:9185".parse().unwrap();
        let registry = mysten_metrics::start_prometheus_server(binding_address).default_registry();
        Metrics::new(&registry)
    }

    fn service() -> Router {
        let version = Version::for_testing();
        let metrics = metrics();
        let cancellation_token = CancellationToken::new();
        let connection_config = ConnectionConfig::default();
        let service_config = ServiceConfig::default();
        let state = AppState::new(
            connection_config.clone(),
            service_config.clone(),
            metrics.clone(),
            cancellation_token.clone(),
            version,
        );

        Router::new()
            .route("/", get(|| async { "Hello, Versioning!" }))
            .route("/graphql", get(|| async { "Hello, Versioning!" }))
            .layer(middleware::from_fn_with_state(
                state.version,
                set_version_middleware,
            ))
    }

    fn graphql_request() -> Request<Body> {
        Request::builder()
            .uri("/graphql")
            .body(Body::empty())
            .unwrap()
    }

    fn plain_request() -> Request<Body> {
        Request::builder().uri("/").body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn default_graphql_route() {
        let version = Version::for_testing();
        let service = service();
        let response = service.oneshot(graphql_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
    }

    #[tokio::test]
    async fn default_plain_route() {
        let version = Version::for_testing();
        let service = service();
        let response = service.oneshot(plain_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
    }
}
