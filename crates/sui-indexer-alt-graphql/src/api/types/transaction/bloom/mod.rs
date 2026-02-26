// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use anyhow::Context as _;
use async_graphql::Context;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use diesel::sql_types::Integer;
use diesel::sql_types::SmallInt;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_indexer_alt_reader::kv_loader::TransactionEventsContents;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::blocked::BlockedBloomProbe;
use sui_indexer_alt_schema::blooms::bloom::BloomProbe;
use sui_indexer_alt_schema::cp_bloom_blocks::CP_BLOCK_SIZE;
use sui_indexer_alt_schema::cp_bloom_blocks::CpBlockedBloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_index;
use sui_indexer_alt_schema::cp_blooms::CpBloomFilter;
use sui_pg_db::query::Query;
use sui_sql_macro::query;
use sui_types::base_types::ExecutionDigests;
use sui_types::digests::TransactionDigest;

use crate::api::types::event::CEvent;
use crate::api::types::event::Event;
use crate::api::types::event::EventCursor;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;

/// Multiplier to page limit to adjust for bloom filter false positives.
const OVERFETCH_MULTIPLIER: f64 = 1.2;

pub(super) type TransactionsBySequenceNumbers =
    BTreeMap<u64, (TransactionDigest, TransactionContents)>;

pub(crate) async fn transactions(
    ctx: &Context<'_>,
    filter: &TransactionFilter,
    page: &Page<CTransaction>,
    cp_bounds: RangeInclusive<u64>,
) -> Result<TransactionsBySequenceNumbers, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;

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
        candidate_cps(ctx, &filter_values, cp_lo, cp_hi, page).await?
    };

    if candidate_cps.is_empty() {
        return Ok(BTreeMap::new());
    }

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

pub(super) type EventsBySequenceNumbers = BTreeMap<EventCursor, Event>;

/// The map of events that might match the filter criteria in `cp_bounds` checkpoints keyed by EventCursor.
pub(crate) async fn events(
    ctx: &Context<'_>,
    scope: &Scope,
    filter: &EventFilter,
    page: &Page<CEvent>,
    cp_bounds: RangeInclusive<u64>,
) -> Result<EventsBySequenceNumbers, RpcError> {
    let kv_loader: &KvLoader = ctx.data()?;

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
        candidate_cps(ctx, &filter_values, cp_lo, cp_hi, page).await?
    };

    if candidate_cps.is_empty() {
        return Ok(BTreeMap::new());
    }

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

    let digests: Vec<_> = sequenced_tx_digests
        .iter()
        .map(|(_, digest)| *digest)
        .collect();
    let events_by_digest: std::collections::HashMap<_, TransactionEventsContents> = kv_loader
        .load_many_transaction_events(digests)
        .await
        .context("Failed to load transaction events")?;

    let mut result = BTreeMap::new();
    for (tx_sequence_number, transaction_digest) in sequenced_tx_digests {
        let contents = events_by_digest
            .get(&transaction_digest)
            .with_context(|| format!("Missing events for transaction {transaction_digest}"))?;
        let timestamp_ms = contents.timestamp_ms();
        for (idx, native) in contents.events()?.into_iter().enumerate() {
            let sequence_number = idx as u64;
            result.insert(
                EventCursor {
                    tx_sequence_number,
                    ev_sequence_number: sequence_number,
                },
                Event {
                    scope: scope.clone(),
                    native,
                    transaction_digest,
                    sequence_number,
                    timestamp_ms,
                },
            );
        }
    }
    Ok(result)
}

/// The checkpoints that might contain the filter criteria.
///
/// Does a coarse filter over checkpoints ranges using cp_bloom_blocks,
/// then a finer filter over those ranges for checkpoint matches using cp_blooms.
async fn candidate_cps<C>(
    ctx: &Context<'_>,
    filter_values: &[[u8; 32]],
    cp_lo: u64,
    cp_hi_inclusive: u64,
    page: &Page<C>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database for bloom filter scan")?;

    let cp_block_lo = cp_block_index(cp_lo);
    let cp_block_hi_inclusive = cp_block_index(cp_hi_inclusive);

    // Block index and probe for each block in the range. Seeds vary per block, so we must
    // construct probes for each block.
    let probes_by_block = (cp_block_lo..=cp_block_hi_inclusive).flat_map(|id| {
        CpBlockedBloomFilter::probe(id as u128, filter_values)
            .into_iter()
            .map(move |probe| (id, probe))
    });

    let q_block_probes = cp_block_probes_sql(probes_by_block);
    let q_bloom_check = cp_bloom_check_sql(&CpBloomFilter::probe(filter_values));

    let block_size = CP_BLOCK_SIZE as i64;
    let adjusted_limit = (page.limit_with_overhead() as f64 * OVERFETCH_MULTIPLIER) as i64;

    // For each unique (cp_block_index, bloom_block_index) probe pair, fetch the bloom block
    // row once via index lookup, then check all bit probes against it.
    let matched_blocks = query!(
        r#"
        SELECT DISTINCT
            cp_bloom_blocks.cp_block_index,
            cp_bloom_blocks.cp_block_index * {BigInt} AS cp_lo,
            cp_bloom_blocks.cp_block_index * {BigInt} + {BigInt} - 1 AS cp_hi_inclusive
        FROM
            (SELECT DISTINCT cp_block_index, bloom_block_index FROM block_byte_probes) unique_probes
        JOIN cp_bloom_blocks USING (cp_block_index, bloom_block_index)
        WHERE NOT EXISTS (
            SELECT 1
            FROM block_byte_probes
            WHERE block_byte_probes.cp_block_index = cp_bloom_blocks.cp_block_index
                AND block_byte_probes.bloom_block_index = cp_bloom_blocks.bloom_block_index
                AND (get_byte(
                    cp_bloom_blocks.bloom_filter,
                    block_byte_probes.byte_pos % length(cp_bloom_blocks.bloom_filter)
                ) & block_byte_probes.bit_mask) <> block_byte_probes.bit_mask
        )
        ORDER BY
            cp_lo {}
        LIMIT
            {BigInt}
        "#,
        block_size,
        block_size,
        block_size,
        page.order_by_direction(),
        adjusted_limit,
    );

    // For each matched block, scan cp_blooms until we have adjusted_limit checkpoints that
    // match the probe.
    let query = query!(
        r#"
        WITH
        block_byte_probes AS ({}),

        matched_blocks AS ({})

        SELECT
            cp_sequence_number::BIGINT
        FROM
            matched_blocks
        CROSS JOIN LATERAL (
            SELECT
                cp_sequence_number
            FROM
                cp_blooms
            WHERE
                cp_sequence_number BETWEEN GREATEST(matched_blocks.cp_lo, {BigInt})
                    AND LEAST(matched_blocks.cp_hi_inclusive, {BigInt})
                AND {}
            ORDER BY
                cp_sequence_number {}
        ) cp_blooms
        LIMIT
            {BigInt}
        "#,
        q_block_probes,
        matched_blocks,
        cp_lo as i64,
        cp_hi_inclusive as i64,
        q_bloom_check,
        page.order_by_direction(),
        adjusted_limit,
    );

    #[derive(QueryableByName)]
    struct CpResult {
        #[diesel(sql_type = BigInt)]
        cp_sequence_number: i64,
    }

    let results: Vec<CpResult> = conn
        .results(query)
        .await
        .context("Failed to execute bloom filter scan query")?;
    Ok(results
        .into_iter()
        .map(|r| r.cp_sequence_number as u64)
        .collect())
}

/// SQL fragment that produces rows of probes (cp_block_index, bloom_block_index, byte_pos, bit_mask) using UNNEST
fn cp_block_probes_sql(probes: impl Iterator<Item = (i64, BlockedBloomProbe)>) -> Query<'static> {
    let mut cp_block_indices = vec![];
    let mut bloom_indicies = vec![];
    let mut byte_offsets = vec![];
    let mut bit_masks = vec![];

    for (cp_block_index, blocked_probe) in probes {
        for &(offset, mask) in &blocked_probe.probe.bit_probes {
            cp_block_indices.push(cp_block_index);
            bloom_indicies.push(blocked_probe.block_idx as i16);
            byte_offsets.push(offset as i32);
            bit_masks.push(mask as i32);
        }
    }

    query!(
        r#"
        SELECT
            UNNEST({Array<BigInt>}) cp_block_index,
            UNNEST({Array<SmallInt>}) bloom_block_index,
            UNNEST({Array<Integer>}) byte_pos,
            UNNEST({Array<Integer>}) bit_mask
        "#,
        cp_block_indices,
        bloom_indicies,
        byte_offsets,
        bit_masks,
    )
}

/// Check if all filter values are present in a checkpoint's bloom filter.
fn cp_bloom_check_sql(probe: &BloomProbe) -> Query<'static> {
    if probe.bit_probes.is_empty() {
        return query!("TRUE");
    }

    let mut condition = query!("TRUE");
    for &(offset, mask) in &probe.bit_probes {
        condition += query!(
            " AND (get_byte(cp_blooms.bloom_filter, {Integer} % length(cp_blooms.bloom_filter)) & {Integer}) = {Integer}",
            offset as i32,
            mask as i32,
            mask as i32,
        );
    }
    condition
}
