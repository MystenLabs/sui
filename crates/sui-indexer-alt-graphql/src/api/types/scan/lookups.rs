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

/// Load transaction digests from checkpoints.
pub(crate) async fn load_digests(
    ctx: &Context<'_>,
    candidate_cps: Vec<u64>,
) -> Result<DigestsByCheckpoint, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;
    Ok(kv_loader
        .load_many_checkpoints_transactions(candidate_cps)
        .await
        .context("Failed to load checkpoint transactions")?)
}

/// Load transaction contents from digests.
pub(super) async fn load_transactions(
    ctx: &Context<'_>,
    digests: Vec<TransactionDigest>,
) -> Result<TransactionsByDigest, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;
    Ok(kv_loader
        .load_many_transactions(digests)
        .await
        .context("Failed to load transactions")?)
}

/// Load events from digests.
pub(crate) async fn load_events(
    ctx: &Context<'_>,
    digests: Vec<TransactionDigest>,
) -> Result<EventsByDigest, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;
    Ok(kv_loader
        .load_many_transaction_events(digests)
        .await
        .context("Failed to load transaction events")?)
}
