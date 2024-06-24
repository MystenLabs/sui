// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    extract::{Path, State},
    headers,
    http::{HeaderName, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    config::Version,
    error::{code, graphql_error_response},
};

pub(crate) static VERSION_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-version");

pub(crate) struct SuiRpcVersion(Vec<u8>, Vec<Vec<u8>>);
const NAMED_VERSIONS: [&str; 3] = ["beta", "legacy", "stable"];

impl headers::Header for SuiRpcVersion {
    fn name() -> &'static HeaderName {
        &VERSION_HEADER
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        let mut values = values.map(|v| v.as_bytes().to_owned());
        let Some(value) = values.next() else {
            // No values for this header -- it doesn't exist.
            return Err(headers::Error::invalid());
        };

        // Extract the header values as bytes.  Distinguish the first value as we expect there to be
        // just one under normal operation.  Do not attempt to parse the value, as a header parsing
        // failure produces a generic error.
        Ok(SuiRpcVersion(value, values.collect()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, _values: &mut E) {
        unimplemented!()
    }
}

/// Middleware to check for the existence of a version constraint in the request header, and confirm
/// that this instance of the RPC matches that version constraint.  Each RPC instance only supports
/// one version of the RPC software, and it is the responsibility of the load balancer to make sure
/// version constraints are met.
pub(crate) async fn check_version_middleware<B>(
    version: Option<Path<String>>,
    State(service_version): State<Version>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    let Some(Path(version)) = version else {
        return next.run(request).await;
    };

    if NAMED_VERSIONS.contains(&version.as_str()) || version.is_empty() {
        return next.run(request).await;
    }
    let Some((year, month)) = parse_version(&version) else {
        return (
                StatusCode::BAD_REQUEST,
                graphql_error_response(
                    code::BAD_REQUEST,
                    format!(
                        "Failed to parse version path: {version}. Expected either a `beta | legacy | stable` \
                    version or <YEAR>.<MONTH> version.",
                    ),
                ),
            )
                .into_response();
    };

    if year != service_version.year || month != service_version.month {
        return (
            StatusCode::MISDIRECTED_REQUEST,
            graphql_error_response(
                code::INTERNAL_SERVER_ERROR,
                format!("Version '{version}' not supported."),
            ),
        )
            .into_response();
    }
    next.run(request).await
}

/// Mark every outgoing response with a header indicating the precise version of the RPC that was
/// used (including the patch version and sha).
pub(crate) async fn set_version_middleware<B>(
    State(version): State<Version>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        VERSION_HEADER.clone(),
        HeaderValue::from_static(version.full),
    );
    response
}

/// Split a `version` string into two parts (year and month) separated by a ".".
///
/// Confirms that the version specifier contains exactly two components, and that both
/// components are entirely comprised of digits.
fn parse_version(version: &str) -> Option<(&str, &str)> {
    let mut parts = version.split('.');
    let year = parts.next()?;
    let month = parts.next()?;

    if year.is_empty() || month.is_empty() {
        return None;
    }

    (parts.next().is_none()
        && year.chars().all(|c| c.is_ascii_digit())
        && month.chars().all(|c| c.is_ascii_digit()))
    .then_some((year, month))
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
    use expect_test::expect;
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
            .route("/:version", get(|| async { "Hello, Versioning!" }))
            .route("/graphql", get(|| async { "Hello, Versioning!" }))
            .route("/graphql/:version", get(|| async { "Hello, Versioning!" }))
            .layer(middleware::from_fn_with_state(
                state.version,
                check_version_middleware,
            ))
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

    fn version_request(version: &str) -> Request<Body> {
        if version.is_empty() {
            return plain_request();
        }
        Request::builder()
            .uri(format!("/graphql/{}", version))
            .body(Body::empty())
            .unwrap()
    }

    async fn response_body(response: Response) -> String {
        let bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(bytes.as_ref()).unwrap();
        serde_json::to_string_pretty(&value).unwrap()
    }

    #[tokio::test]
    async fn successful() {
        let version = Version::for_testing();
        let major_version = format!("{}.{}", version.year, version.month);
        let service = service();
        let response = service
            .oneshot(version_request(&major_version))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
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
    async fn named_version() {
        let version = Version::for_testing();
        let service = service();
        for named_version in NAMED_VERSIONS {
            let response = service
                .clone()
                .oneshot(version_request(named_version))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get(&VERSION_HEADER),
                Some(&HeaderValue::from_static(version.full))
            );
        }
    }

    #[tokio::test]
    async fn default_version() {
        let version = Version::for_testing();
        let service = service();
        let response = service.oneshot(plain_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
    }

    #[tokio::test]
    async fn wrong_path() {
        let version = Version::for_testing();
        let service = service();
        let response = service.oneshot(version_request("")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
    }

    #[tokio::test]
    async fn incompatible_version() {
        let version = Version::for_testing();
        let service = service();
        let response = service.oneshot(version_request("0.0")).await.unwrap();

        assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );

        let expect = expect![[r#"
            {
              "data": null,
              "errors": [
                {
                  "message": "Version '0.0' not supported.",
                  "extensions": {
                    "code": "INTERNAL_SERVER_ERROR"
                  }
                }
              ]
            }"#]];
        expect.assert_eq(&response_body(response).await);
    }

    #[tokio::test]
    async fn not_a_version() {
        let version = Version::for_testing();
        let service = service();
        let response = service
            .oneshot(version_request("not-a-version"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );

        let expect = expect![[r#"
            {
              "data": null,
              "errors": [
                {
                  "message": "Failed to parse version path: not-a-version. Expected either a `beta | legacy | stable` version or <YEAR>.<MONTH> version.",
                  "extensions": {
                    "code": "BAD_REQUEST"
                  }
                }
              ]
            }"#]];
        expect.assert_eq(&response_body(response).await);
    }
}
