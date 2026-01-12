// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context as _;
use async_graphql::Context;
use sui_indexer_alt_reader::checkpoints::CheckpointKey;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_indexer_alt_reader::kv_loader::TransactionEventsContents;
use sui_types::digests::TransactionDigest;

use crate::error::RpcError;

pub(crate) type DigestsByCheckpoint = HashMap<CheckpointKey, Vec<TransactionDigest>>;
pub(super) type TransactionsByDigest = HashMap<TransactionDigest, TransactionContents>;
pub(crate) type EventsByDigest = HashMap<TransactionDigest, TransactionEventsContents>;

/// Load transaction digests for checkpoints. Shared by transaction and event scanning.
pub(crate) async fn load_digests(
    ctx: &Context<'_>,
    candidate_cps: &[u64],
) -> Result<DigestsByCheckpoint, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;
    kv_loader
        .load_many_checkpoints_transactions(candidate_cps.to_vec())
        .await
        .context("Failed to load checkpoint transactions")
        .map_err(Into::into)
}

/// Load transaction contents from digests.
pub(super) async fn load_transactions(
    ctx: &Context<'_>,
    digests: &DigestsByCheckpoint,
) -> Result<TransactionsByDigest, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;
    let tx_digests: Vec<_> = digests.values().flatten().copied().collect();
    kv_loader
        .load_many_transactions(tx_digests)
        .await
        .context("Failed to load transactions")
        .map_err(Into::into)
}

/// Load events from digests.
pub(crate) async fn load_events(
    ctx: &Context<'_>,
    digests: &DigestsByCheckpoint,
) -> Result<EventsByDigest, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;
    let tx_digests: Vec<_> = digests.values().flatten().copied().collect();
    kv_loader
        .load_many_transaction_events(tx_digests)
        .await
        .context("Failed to load transaction events")
        .map_err(Into::into)
}
