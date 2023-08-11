// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::error::{code, graphql_error};

const VERSION_HEADER: &str = "X-Sui-RPC-Version";

const RPC_VERSION_FULL: &str = env!("CARGO_PKG_VERSION");
const RPC_VERSION_YEAR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const RPC_VERSION_MONTH: &str = env!("CARGO_PKG_VERSION_MINOR");

/// Middleware to check for the existence of a version constraint in the request header, and confirm
/// that this instance of the RPC matches that version constraint.  Each RPC instance only supports
/// one version of the RPC software, and it is the responsibility of the load balancer to make sure
/// version constraints are met.
pub(crate) async fn check_version_middleware<B>(
    headers: HeaderMap,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    if let Some(req_version) = headers.get(VERSION_HEADER) {
        let Ok(req_version) = req_version.to_str() else {
            return (
                StatusCode::BAD_REQUEST,
                graphql_error(
                    code::BAD_REQUEST,
                    format!("Failed to parse {VERSION_HEADER}: Not an ASCII string."),
                ),
            ).into_response();
        };

        let Some((year, month)) = parse_version(req_version) else {
            return (
                StatusCode::BAD_REQUEST,
                graphql_error(
                    code::BAD_REQUEST,
                    format!(
                        "Failed to parse {VERSION_HEADER}: '{req_version}' not a valid \
                         <YEAR>.<MONTH> version.",
                    ),
                ),
            ).into_response();
        };

        if year != RPC_VERSION_YEAR || month != RPC_VERSION_MONTH {
            return (
                StatusCode::MISDIRECTED_REQUEST,
                graphql_error(
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
/// used (including the patch version).
pub(crate) async fn set_version_middleware<B>(request: Request<B>, next: Next<B>) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(VERSION_HEADER, HeaderValue::from_static(RPC_VERSION_FULL));
    response
}

/// Split a `version` string into two parts (year and month) separated by a ".".
///
/// Confirms that the version specifier contains exactly two components, and that both
/// components are entirely comprised of digits.
fn parse_version(version: &str) -> Option<(&str, &str)> {
    let mut parts = version.split(".");
    let year = parts.next()?;
    let month = parts.next()?;

    (parts.next().is_none()
        && year.chars().all(|c| c.is_ascii_digit())
        && month.chars().all(|c| c.is_ascii_digit()))
    .then_some((year, month))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, middleware, routing::get, Router};
    use expect_test::expect;
    use tower::ServiceExt;

    use super::*;

    fn service() -> Router {
        Router::new()
            .route("/", get(|| async { "Hello, Versioning!" }))
            .layer(middleware::from_fn(check_version_middleware))
            .layer(middleware::from_fn(set_version_middleware))
    }

    fn plain_request() -> Request<Body> {
        Request::builder().uri("/").body(Body::empty()).unwrap()
    }

    fn header_request(name: &'static str, value: &[u8]) -> Request<Body> {
        let mut request = plain_request();
        let headers = request.headers_mut();
        headers.insert(name, HeaderValue::from_bytes(value).unwrap());
        request
    }

    async fn response_body(response: Response) -> String {
        let bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(bytes.as_ref()).unwrap();
        serde_json::to_string_pretty(&value).unwrap()
    }

    #[tokio::test]
    async fn successful() {
        let version = format!("{RPC_VERSION_YEAR}.{RPC_VERSION_MONTH}");
        let service = service();
        let response = service
            .oneshot(header_request("X-Sui-RPC-Version", version.as_bytes()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("X-Sui-RPC-Version"),
            Some(&HeaderValue::from_static(RPC_VERSION_FULL)),
        );
    }

    #[tokio::test]
    async fn case_insensitive() {
        let version = format!("{RPC_VERSION_YEAR}.{RPC_VERSION_MONTH}");
        let service = service();
        let response = service
            .oneshot(header_request("x-sUI-RpC-VeRSion", version.as_bytes()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("X-Sui-RPC-Version"),
            Some(&HeaderValue::from_static(RPC_VERSION_FULL)),
        );
    }

    #[tokio::test]
    async fn default_version() {
        let service = service();
        let response = service.oneshot(plain_request()).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("X-Sui-RPC-Version"),
            Some(&HeaderValue::from_static(RPC_VERSION_FULL)),
        );
    }

    #[tokio::test]
    async fn incompatible_version() {
        let next_year = 1 + RPC_VERSION_YEAR.parse::<u16>().expect("a number");
        let version = format!("{next_year}.{RPC_VERSION_MONTH}");
        let service = service();
        let response = service
            .oneshot(header_request("X-Sui-RPC-Version", version.as_bytes()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(
            response.headers().get("X-Sui-RPC-Version"),
            Some(&HeaderValue::from_static(RPC_VERSION_FULL)),
        );

        let expect = expect![[r#"
            {
              "data": null,
              "errors": [
                {
                  "message": "Version '1.1' not supported.",
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
        let service = service();
        let response = service
            .oneshot(header_request("X-Sui-RPC-Version", b"not-a-version"))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get("X-Sui-RPC-Version"),
            Some(&HeaderValue::from_static(RPC_VERSION_FULL)),
        );

        let expect = expect![[r#"
            {
              "data": null,
              "errors": [
                {
                  "message": "Failed to parse X-Sui-RPC-Version: 'not-a-version' not a valid <YEAR>.<MONTH> version.",
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
        let service = service();
        let response = service
            .oneshot(header_request(
                "X-Sui-RPC-Version",
                &[0xf1, 0xf2, 0xf3, 0xf4],
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get("X-Sui-RPC-Version"),
            Some(&HeaderValue::from_static(RPC_VERSION_FULL)),
        );

        let expect = expect![[r#"
            {
              "data": null,
              "errors": [
                {
                  "message": "Failed to parse X-Sui-RPC-Version: Not an ASCII string.",
                  "extensions": {
                    "code": "BAD_REQUEST"
                  }
                }
              ]
            }"#]];
        expect.assert_eq(&response_body(response).await);
    }
}
