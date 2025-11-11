// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context as _;
use async_graphql::Context;
use sui_indexer_alt_reader::checkpoints::CheckpointKey;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_types::digests::TransactionDigest;

use crate::error::RpcError;

pub(super) type DigestsByCheckpoint = HashMap<CheckpointKey, Vec<TransactionDigest>>;
pub(super) type TransactionsByDigest = HashMap<TransactionDigest, TransactionContents>;

pub(super) async fn load_transactions(
    ctx: &Context<'_>,
    candidate_cps: &[u64],
) -> Result<(DigestsByCheckpoint, TransactionsByDigest), RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;

    let digests = kv_loader
        .load_many_checkpoints_transactions(candidate_cps.to_vec())
        .await
        .context("Failed to load checkpoint transactions")?;

    let tx_digests_to_load: Vec<_> = digests.values().flatten().copied().collect();
    let native_transactions = kv_loader
        .load_many_transactions(tx_digests_to_load)
        .await
        .context("Failed to load transactions")?;

    Ok((digests, native_transactions))
}
