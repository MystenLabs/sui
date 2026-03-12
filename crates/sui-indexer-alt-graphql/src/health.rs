// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::ensure;
use async_graphql::indexmap::IndexMap;
use axum::Extension;
use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response as AxumResponse;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use tokio::net::TcpStream;
use url::Url;

use crate::WatermarksLock;
use crate::config::HealthConfig;

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
#[derive(Serialize, Default)]
pub(crate) struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_lag_ms: Option<u64>,

    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pipelines: IndexMap<String, u64>,

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
    let mut resp = Response::default();

    if let Err(e) = check_connection(&db_url).await {
        resp.errors.push(format!("DB probe failed: {e}"));
    }

    let (checkpoint_lag_ms, pipelines) = match check_watermarks(&watermarks).await {
        Ok(watermark) => watermark,
        Err(e) => {
            resp.errors.push(e.to_string());
            return resp;
        }
    };

    let max_lag = params
        .max_checkpoint_lag_ms
        .unwrap_or(config.max_checkpoint_lag.as_millis() as u64);

    if checkpoint_lag_ms > max_lag {
        resp.errors.push("Watermark lag is too high".to_owned());
    }

    resp.checkpoint_lag_ms = Some(checkpoint_lag_ms);
    resp.pipelines = pipelines;
    resp
}

/// Returns the lag between the latest checkpoint the indexer is aware of and the current time.
async fn check_watermarks(
    watermarks: &WatermarksLock,
) -> anyhow::Result<(u64, IndexMap<String, u64>)> {
    let now = Utc::now();
    let watermarks = watermarks.read().await;

    ensure!(
        watermarks.initialized(),
        "Watermark has not been initialized"
    );

    let mut pipeline_lags: Vec<(String, u64)> = watermarks
        .per_pipeline()
        .iter()
        .map(|(pipeline, watermark)| (pipeline.clone(), watermark.lag_ms(now)))
        .collect();

    pipeline_lags.sort_by(|(name_a, lag_a), (name_b, lag_b)| {
        lag_b.cmp(lag_a).then_with(|| name_a.cmp(name_b))
    });

    let pipelines = pipeline_lags.into_iter().collect();
    Ok((watermarks.lag_ms(now), pipelines))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Extension;
    use axum::extract::Query;
    use tokio::sync::RwLock;

    use super::*;

    #[tokio::test]
    async fn test_uninitialized_watermark_is_unhealthy() {
        let response = check(
            Extension(Arc::new(RwLock::new(Arc::new(
                crate::task::watermark::Watermarks::default(),
            )))),
            Extension(HealthConfig::default()),
            Extension(DbProbe(None)),
            Query(Params {
                max_checkpoint_lag_ms: None,
            }),
        )
        .await;

        assert_eq!(response.checkpoint_lag_ms, None);
        assert!(response.pipelines.is_empty());
        assert!(
            response
                .errors
                .contains(&"Watermark has not been initialized".to_owned())
        );
    }
}
