// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helpers for exercising the GraphQL RPC from integration tests: a thin query runner and
//! generic Relay-style connection types, so individual tests only define their node-specific shape.
//!
//! These are conveniences, not a stable shared surface — they happen to cover the connection-style
//! tests written so far. Reuse them when they fit, but if a test needs something different, prefer a
//! helper local to that test's module over bending these to fit. Keeping them minimal is the point.

use anyhow::Context;
use serde::Deserialize;
use serde_json::json;
use url::Url;

/// Relay `PageInfo`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

/// A single Relay connection edge: an opaque `cursor` and its `node`.
#[derive(Debug, Deserialize)]
pub struct Edge<N> {
    pub cursor: String,
    pub node: N,
}

/// A Relay connection over nodes of type `N`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection<N> {
    pub page_info: PageInfo,
    pub edges: Vec<Edge<N>>,
}

/// POST a GraphQL `document` (with `variables`) to a GraphQL server, check for top-level errors,
/// and return the `data` payload. Takes the endpoint URL directly so it works with any cluster
/// (`cluster.graphql_url()`), not a specific cluster type.
pub async fn query(
    graphql_url: &Url,
    document: &str,
    variables: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let response: serde_json::Value = reqwest::Client::new()
        .post(graphql_url.as_str())
        .json(&json!({ "query": document, "variables": variables }))
        .send()
        .await?
        .json()
        .await?;

    if let Some(errors) = response.get("errors") {
        anyhow::bail!("GraphQL errors: {errors}");
    }

    response
        .pointer("/data")
        .cloned()
        .context("Missing data in GraphQL response")
}
