// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Context;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use diesel::sql_types::Integer;
use diesel::sql_types::SmallInt;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::blocked::BlockedBloomProbe;
use sui_indexer_alt_schema::blooms::bloom::BloomProbe;
use sui_indexer_alt_schema::cp_bloom_blocks::CP_BLOCK_SIZE;
use sui_indexer_alt_schema::cp_bloom_blocks::CpBlockedBloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_index;
use sui_indexer_alt_schema::cp_blooms::CpBloomFilter;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::types::transaction::CTransaction;
use crate::error::RpcError;
use crate::pagination::Page;

/// Multiplier to page limit to adjust for bloom filter false positives.
const OVERFETCH_MULTIPLIER: f64 = 1.2;

/// The checkpoints that might contain the filter criteria.
///
/// Does a coarse filter over checkpoints ranges using cp_bloom_blocks,
/// then a finer filter over those ranges for checkpoint matches using cp_blooms.
pub(super) async fn candidate_cps(
    ctx: &Context<'_>,
    filter_values: &[[u8; 32]],
    cp_lo: u64,
    cp_hi_inclusive: u64,
    page: &Page<CTransaction>,
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

    // Keep LIMIT of cp blocks where no probe fails. A probe fails if the bloom block row
    // is missing or the required bit is not set.
    let matched_blocks = query!(
        r#"
        -- Pre-fetch each bloom block once, so the NOT EXISTS check
        -- doesn't re-lookup the same row for every bit probe to reduce IO.
        WITH block_lookup AS MATERIALIZED (
            SELECT
                p.cp_block_index,
                p.bloom_idx,
                bb.bloom_filter
            FROM
                (SELECT DISTINCT cp_block_index, bloom_idx FROM block_bit_probes) p
            LEFT JOIN cp_bloom_blocks bb
                ON bb.cp_block_index = p.cp_block_index
                AND bb.bloom_block_index = p.bloom_idx
        )
        SELECT
            bp.cp_block_index,
            bp.cp_block_index * {BigInt} AS cp_lo,
            bp.cp_block_index * {BigInt} + {BigInt} - 1 AS cp_hi_inclusive
        FROM
            (SELECT DISTINCT cp_block_index FROM block_bit_probes) bp
        WHERE
            NOT EXISTS (
                SELECT 1
                FROM
                    block_bit_probes p
                JOIN block_lookup bl
                    ON bl.cp_block_index = p.cp_block_index
                    AND bl.bloom_idx = p.bloom_idx
                WHERE
                    p.cp_block_index = bp.cp_block_index
                    AND (bl.bloom_filter IS NULL
                        OR (p.bit_mask <> get_byte(
                            bl.bloom_filter,
                            p.byte_pos % length(bl.bloom_filter))
                        ))
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

    // For each matched block, scan cp_blooms by index until we have adjusted_limit of checkpoints that match the probe.
    let query = query!(
        r#"
        WITH
            block_bit_probes AS ({})
            , matched_blocks AS ({})
        SELECT
            cb.cp_sequence_number::BIGINT
        FROM
            matched_blocks mb
        CROSS JOIN LATERAL (
            SELECT
                cb.cp_sequence_number
            FROM
                cp_blooms cb
            WHERE
                cb.cp_sequence_number BETWEEN GREATEST(mb.cp_lo, {BigInt}) AND LEAST(mb.cp_hi_inclusive, {BigInt})
                AND {}
            ORDER BY
                cb.cp_sequence_number {}
        ) cb
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

/// SQL fragment that produces rows of probes (cp_block_index, bloom_idx, byte_pos, bit_mask) using UNNEST
fn cp_block_probes_sql(probes: impl Iterator<Item = (i64, BlockedBloomProbe)>) -> Query<'static> {
    let mut cp_block_indices = vec![];
    let mut bloom_idxs = vec![];
    let mut byte_offsets = vec![];
    let mut bit_masks = vec![];

    for (cp_block_index, blocked_probe) in probes {
        for &(offset, mask) in &blocked_probe.probe.bit_probes {
            cp_block_indices.push(cp_block_index);
            bloom_idxs.push(blocked_probe.block_idx as i16);
            byte_offsets.push(offset as i32);
            bit_masks.push(mask as i32);
        }
    }

    query!(
        r#"
        SELECT
            UNNEST({Array<BigInt>}) cp_block_index,
            UNNEST({Array<SmallInt>}) bloom_idx,
            UNNEST({Array<Integer>}) byte_pos,
            UNNEST({Array<Integer>}) bit_mask
        "#,
        cp_block_indices,
        bloom_idxs,
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
            " AND (get_byte(cb.bloom_filter, {Integer} % length(cb.bloom_filter)) & {Integer}) = {Integer}",
            offset as i32,
            mask as i32,
            mask as i32,
        );
    }
    condition
}
