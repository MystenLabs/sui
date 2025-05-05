// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, bail};
use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response as AxumResponse},
    Extension, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use url::Url;

use crate::{config::HealthConfig, WatermarksLock};

/// Extension that holds a DB URL to probe as part of health checks.
#[derive(Clone)]
pub(crate) struct DbProbe(pub(crate) Option<Url>);

/// Query params for the health check endpoint.
#[derive(Deserialize)]
pub(crate) struct Params {
    /// customise the max allowed checkpoint lag. If it is omitted, the default lag configured for
    /// the service is used.
    max_checkpoint_lag_ms: Option<u64>,
}

/// Response body for the health check endpoint. This is output as JSON.
#[derive(Serialize)]
pub(crate) struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_lag_ms: Option<u64>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

/// Health check endpoint outputs information about the services health -- how recent its
/// information is, and any health check related errors. The response status code is
/// INTERNAL_SERVER_ERROR if there are any errors, and OK otherwise (if the service is healthy).
pub(crate) async fn check(
    Extension(watermarks): Extension<WatermarksLock>,
    Extension(config): Extension<HealthConfig>,
    Extension(DbProbe(db_url)): Extension<DbProbe>,
    Query(params): Query<Params>,
) -> Response {
    let mut errors = vec![];

    let lag = match check_watermarks(&watermarks).await {
        Ok(lag) => Some(lag),

        Err(e) => {
            errors.push(e.to_string());
            None
        }
    };

    if let Err(e) = check_connection(&db_url).await {
        errors.push(format!("DB probe failed: {e}"));
    }

    let max_lag = params
        .max_checkpoint_lag_ms
        .map(Duration::from_millis)
        .unwrap_or(config.max_checkpoint_lag);

    if lag.is_some_and(|l| l > max_lag) {
        errors.push("Watermark lag is too high".to_owned());
    }

    Response {
        checkpoint_lag_ms: lag.map(|l| l.as_millis() as u64),
        errors,
    }
}

/// Returns the lag between the latest checkpoint the indexer is aware of and the current time.
async fn check_watermarks(watermarks: &WatermarksLock) -> anyhow::Result<Duration> {
    let now = Utc::now();
    let Some(then) = watermarks.read().await.timestamp_hi() else {
        bail!("Invalid watermark timestamp");
    };

    Ok((now - then).to_std().unwrap_or_default())
}

/// Checks that the service can talk to the database.
async fn check_connection(url: &Option<Url>) -> anyhow::Result<()> {
    // No URL configured to probe.
    let Some(url) = url else {
        return Ok(());
    };

    let addrs = url
        .socket_addrs(|| None)
        .map_err(|_| anyhow!("Could not resolve URL"))?;

    TcpStream::connect(addrs.as_slice())
        .await
        .map_err(|_| anyhow!("Failed to establish TCP connection"))?;

    Ok(())
}

impl IntoResponse for Response {
    fn into_response(self) -> AxumResponse {
        let status = if self.errors.is_empty() {
            StatusCode::OK
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };

        (status, Json(self)).into_response()
    }
}
