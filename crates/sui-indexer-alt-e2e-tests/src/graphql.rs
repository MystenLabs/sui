// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helpers for exercising the GraphQL RPC from integration tests: a thin query runner and
//! generic Relay-style connection types, so individual tests only define their node-specific shape.
//!
//! These are conveniences, not a stable shared surface — they happen to cover the connection-style
//! tests written so far. Reuse them when they fit, but if a test needs something different, prefer a
//! helper local to that test's module over bending these to fit. Keeping them minimal is the point.

use anyhow::Context;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use url::Url;

/// Leading byte of every cursor in GraphQL's own format.
const FORMAT_MARKER: u8 = 0x00;

/// Envelope variants, `OwnedCursor` variants, and `Point` variants of GraphQL's cursor format, by
/// BCS variant index.
const TRANSACTION: u8 = 0;
const EVENT: u8 = 1;
const OWNED_POINT: u8 = 0;
const OWNED_LEDGER: u8 = 1;
const POINT_AT: u8 = 0;

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

/// A GraphQL transactions cursor at the transaction at `(checkpoint, transaction_index)`, with no
/// ledger bytes, as GraphQL mints it for a transaction it holds.
pub fn place_tx_cursor(checkpoint: u64, transaction_index: u64) -> String {
    graphql_cursor((
        TRANSACTION,
        OWNED_POINT,
        POINT_AT,
        checkpoint,
        transaction_index,
    ))
}

/// The event counterpart of [`place_tx_cursor`].
pub fn place_event_cursor(checkpoint: u64, transaction_index: u64, event_index: u32) -> String {
    graphql_cursor((
        EVENT,
        OWNED_POINT,
        POINT_AT,
        checkpoint,
        transaction_index,
        event_index,
    ))
}

/// A GraphQL transactions cursor carrying `ledger`, the ledger service's own cursor bytes, that
/// GraphQL could not place (as for a page boundary nothing GraphQL saw bounds).
pub fn unplaced_tx_cursor(ledger: &[u8]) -> String {
    graphql_cursor((TRANSACTION, OWNED_LEDGER, ledger, None::<()>))
}

/// The event counterpart of [`unplaced_tx_cursor`].
pub fn unplaced_event_cursor(ledger: &[u8]) -> String {
    graphql_cursor((EVENT, OWNED_LEDGER, ledger, None::<()>))
}

/// [`FORMAT_MARKER`], then the BCS encoding of `envelope`, in Base64.
fn graphql_cursor(envelope: impl Serialize) -> String {
    Base64::encode(bcs::to_bytes(&(FORMAT_MARKER, envelope)).expect("cursor encodes"))
}
