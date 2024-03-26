// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    extract::State,
    headers,
    http::{HeaderName, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    TypedHeader,
};

use crate::{
    config::Version,
    error::{code, graphql_error_response},
};

pub(crate) static VERSION_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-version");

pub(crate) struct SuiRpcVersion(Vec<u8>, Vec<Vec<u8>>);

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
    user_version: Option<TypedHeader<SuiRpcVersion>>,
    State(version): State<Version>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    if let Some(TypedHeader(SuiRpcVersion(req_version, rest))) = user_version {
        if !rest.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                graphql_error_response(
                    code::BAD_REQUEST,
                    format!("Failed to parse {VERSION_HEADER}: Multiple possible versions found."),
                ),
            )
                .into_response();
        }

        let Ok(req_version) = std::str::from_utf8(&req_version) else {
            return (
                StatusCode::BAD_REQUEST,
                graphql_error_response(
                    code::BAD_REQUEST,
                    format!("Failed to parse {VERSION_HEADER}: Not a UTF8 string."),
                ),
            )
                .into_response();
        };

        let Some((year, month)) = parse_version(req_version) else {
            return (
                StatusCode::BAD_REQUEST,
                graphql_error_response(
                    code::BAD_REQUEST,
                    format!(
                        "Failed to parse {VERSION_HEADER}: '{req_version}' not a valid \
                         <YEAR>.<MONTH> version.",
                    ),
                ),
            )
                .into_response();
        };

        if year != version.year || month != version.month {
            return (
                StatusCode::MISDIRECTED_REQUEST,
                graphql_error_response(
                    code::INTERNAL_SERVER_ERROR,
                    format!("Version '{req_version}' not supported."),
                ),
            )
                .into_response();
        }
    };

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
        let connection_config = ConnectionConfig::ci_integration_test_cfg();
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
            .layer(middleware::from_fn_with_state(
                state.version,
                check_version_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                state.version,
                set_version_middleware,
            ))
    }

    fn plain_request() -> Request<Body> {
        Request::builder().uri("/").body(Body::empty()).unwrap()
    }

    fn header_request(kvps: &[(&HeaderName, &[u8])]) -> Request<Body> {
        let mut request = plain_request();
        let headers = request.headers_mut();
        for (name, value) in kvps {
            headers.append(*name, HeaderValue::from_bytes(value).unwrap());
        }
        request
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
            .oneshot(header_request(&[(
                &VERSION_HEADER,
                major_version.as_bytes(),
            )]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
    }

    #[tokio::test]
    async fn case_insensitive() {
        let version = Version::for_testing();
        let major_version = format!("{}.{}", version.year, version.month);
        let service = service();
        let response = service
            .oneshot(header_request(&[(
                &HeaderName::try_from("x-sUi-RpC-vERSION").unwrap(),
                major_version.as_bytes(),
            )]))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(&VERSION_HEADER),
            Some(&HeaderValue::from_static(version.full))
        );
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
    async fn incompatible_version() {
        let version = Version::for_testing();
        let service = service();
        let response = service
            .oneshot(header_request(&[(&VERSION_HEADER, "0.0".as_bytes())]))
            .await
            .unwrap();

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
    async fn multiple_versions() {
        let version = Version::for_testing();
        let service = service();
        let response = service
            .oneshot(header_request(&[
                (&VERSION_HEADER, "0.0".as_bytes()),
                (&VERSION_HEADER, "1.0".as_bytes()),
            ]))
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
                  "message": "Failed to parse x-sui-rpc-version: Multiple possible versions found.",
                  "extensions": {
                    "code": "BAD_REQUEST"
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
            .oneshot(header_request(&[(&VERSION_HEADER, b"not-a-version")]))
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
                  "message": "Failed to parse x-sui-rpc-version: 'not-a-version' not a valid <YEAR>.<MONTH> version.",
                  "extensions": {
                    "code": "BAD_REQUEST"
                  }
                }
              ]
            }"#]];
        expect.assert_eq(&response_body(response).await);
    }

    #[tokio::test]
    async fn not_a_string() {
        let version = Version::for_testing();
        let service = service();
        let response = service
            .oneshot(header_request(&[(
                &VERSION_HEADER,
                &[0xf1, 0xf2, 0xf3, 0xf4],
            )]))
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
                  "message": "Failed to parse x-sui-rpc-version: Not a UTF8 string.",
                  "extensions": {
                    "code": "BAD_REQUEST"
                  }
                }
              ]
            }"#]];
        expect.assert_eq(&response_body(response).await);
    }
}
