// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::server::{BridgeMetrics, BridgeNodePublicMetadata, BridgeRequestHandlerTrait};
use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// we expect that some nodes may present an auth challenge that we will attempt to validate
pub async fn expect_client_challenge(
    Extension(metrics): Extension<Arc<BridgeMetrics>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    metrics
        .middleware_ops
        .with_label_values(&["expect_client_challenge", "challenger-accepted"])
        .inc();
    Ok(next.run(request).await)
}
