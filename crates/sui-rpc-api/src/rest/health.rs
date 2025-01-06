// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ApiEndpoint, RouteHandler};
use crate::RpcService;
use axum::extract::{Query, State};

/// Perform a service health check
///
/// By default the health check only verifies that the latest checkpoint can be fetched from the
/// node's store before returning a 200. Optionally the `threshold_seconds` parameter can be
/// provided to test for how up to date the node needs to be to be considered healthy.
pub struct HealthCheck;

impl ApiEndpoint<RpcService> for HealthCheck {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/-/health"
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), health)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Threshold {
    /// The threshold, or delta, between the server's system time and the timestamp in the most
    /// recently executed checkpoint for which the server is considered to be healthy.
    ///
    /// If not provided, the server will be considered healthy if it can simply fetch the latest
    /// checkpoint from its store.
    pub threshold_seconds: Option<u32>,
}

pub async fn health(
    Query(Threshold { threshold_seconds }): Query<Threshold>,
    State(state): State<RpcService>,
) -> impl axum::response::IntoResponse {
    match state.health_check(threshold_seconds) {
        Ok(()) => (axum::http::StatusCode::OK, "up"),
        Err(_) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "down"),
    }
}
