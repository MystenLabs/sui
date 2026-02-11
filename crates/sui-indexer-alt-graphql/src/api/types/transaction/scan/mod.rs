// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod bloom;

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use anyhow::Context as _;
use async_graphql::Context;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;
use sui_types::base_types::ExecutionDigests;
use sui_types::digests::TransactionDigest;

use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::error::RpcError;
use crate::pagination::Page;

pub(super) type TransactionsBySequenceNumbers =
    BTreeMap<u64, (TransactionDigest, NativeTransactionContents)>;

pub(crate) async fn transactions(
    ctx: &Context<'_>,
    filter: &TransactionFilter,
    page: &Page<CTransaction>,
    cp_bounds: RangeInclusive<u64>,
) -> Result<TransactionsBySequenceNumbers, RpcError> {
    let (cp_lo, cp_hi) = (*cp_bounds.start(), *cp_bounds.end());
    let filter_values = filter.bloom_probe_values();
    let candidate_cps = if filter_values.is_empty() {
        let limit = page.limit_with_overhead();
        if page.is_from_front() {
            (cp_lo..=cp_hi).take(limit).collect()
        } else {
            (cp_lo..=cp_hi).rev().take(limit).collect()
        }
    } else {
        bloom::candidate_cps(ctx, &filter_values, cp_lo, cp_hi, page).await?
    };

    if candidate_cps.is_empty() {
        return Ok(BTreeMap::new());
    }

    transactions_by_sequence_numbers(ctx, &candidate_cps).await
}

/// TODO: Refactor KV Loader from enum wrapping per-backend DataLoaders into a raw reader enum (KvReader) wrapped by a single DataLoader
///       This function can then be kv_loader.load_many(CheckpointTransactionsKey).
/// Load checkpoints by cp_sequence_number, then fetch all their transactions from KV,
/// returning them keyed by global transaction sequence number.
pub(super) async fn transactions_by_sequence_numbers(
    ctx: &Context<'_>,
    candidate_cps: &[u64],
) -> Result<TransactionsBySequenceNumbers, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;

    let checkpoints = kv_loader
        .load_many_checkpoints(candidate_cps.to_vec())
        .await
        .context("Failed to load checkpoint transactions")?;

    let sequenced_tx_digests: Vec<_> = checkpoints
        .into_values()
        .flat_map(|(summary, content, _)| {
            content
                .enumerate_transactions(&summary)
                .map(|(tx_seq, &ExecutionDigests { transaction, .. })| (tx_seq, transaction))
                .collect::<Vec<_>>()
        })
        .collect();

    let digests = sequenced_tx_digests
        .iter()
        .map(|(_, digest)| *digest)
        .collect();
    let mut transactions_by_digest = kv_loader
        .load_many_transactions(digests)
        .await
        .context("Failed to load transactions")?;

    sequenced_tx_digests
        .into_iter()
        .map(|(tx_seq, digest)| -> Result<_, RpcError> {
            let contents = transactions_by_digest
                .remove(&digest)
                .with_context(|| format!("Failed to fetch Transaction with digest {digest}"))?;
            Ok((tx_seq, (digest, contents)))
        })
        .collect()
}
