// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    body::Body,
    extract::State,
    http::{HeaderName, HeaderValue, Request},
    middleware::Next,
    response::Response,
};

pub(crate) static VERSION_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-version");

/// Extension wrapping to make the version available to the middleware.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Version(pub &'static str);

/// Mark every outgoing response with a header indicating the precise version of the RPC that was
/// used (including that patch version and SHA).
pub(crate) async fn set_version(
    State(Version(version)): State<Version>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(VERSION_HEADER.clone(), HeaderValue::from_static(version));
    response
}
