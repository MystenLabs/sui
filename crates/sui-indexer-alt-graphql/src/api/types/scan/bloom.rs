// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Context;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use diesel::sql_types::Integer;
use itertools::Itertools;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::blocked::BlockedBloomProbe;
use sui_indexer_alt_schema::blooms::bloom::BloomProbe;
use sui_indexer_alt_schema::cp_bloom_blocks::CP_BLOCK_SIZE;
use sui_indexer_alt_schema::cp_bloom_blocks::CpBlockedBloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_index;
use sui_indexer_alt_schema::cp_blooms::CpBloomFilter;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::types::transaction::SCTransaction;
use crate::error::RpcError;
use crate::pagination::Page;

/// Multiplier to page limit to adjust for bloom filter false positives.
const OVERFETCH_MULTIPLIER: f64 = 1.2;

#[derive(QueryableByName)]
struct CpResult {
    #[diesel(sql_type = BigInt)]
    cp_sequence_number: i64,
}

/// The checkpoints that might contain the filter criteria.
///
/// Does a coarse filter over checkpoints ranges using cp_bloom_blocks,
/// then a finer filter over those ranges for checkpoint matches using cp_blooms.
pub(super) async fn candidate_cps(
    ctx: &Context<'_>,
    filter_values: &[[u8; 32]],
    cp_lo: u64,
    cp_hi_inclusive: u64,
    page: &Page<SCTransaction>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database for bloom filter scan")?;

    let cp_block_lo = cp_block_index(cp_lo);
    let cp_block_hi = cp_block_index(cp_hi_inclusive);

    // Block index and probe for each block in the range. Seeds vary per block, so we must
    // construct probes for each block.
    let block_probes = (cp_block_lo..=cp_block_hi).flat_map(|id| {
        CpBlockedBloomFilter::probe(id as u128, filter_values)
            .into_iter()
            .map(move |probe| (id, probe))
    });

    let cp_bloom_blocks_condition = cp_bloom_block_probes_fragment(block_probes);
    let cp_bloom_condition = cp_bloom_condition_fragment(&CpBloomFilter::probe(filter_values));

    let fpr_adjusted_limit = (page.limit_with_overhead() as f64 * OVERFETCH_MULTIPLIER) as i64;

    // Inline table of probes: which bloom blocks to check and what bits must be set.
    let condition_data = query!(
        "condition_data(cp_block_index, bloom_idx, byte_pos, bit_masks) AS (VALUES {})",
        cp_bloom_blocks_condition,
    );

    // Find a page of checkpoint blocks where all probes matched.
    //
    // Uses two NOT EXISTS to express:
    //   "keep cp_block_index where every probe has a matching bloom block."
    // Outer NOT EXISTS: true when all probes passed for this cp_block_index.
    // Inner NOT EXISTS: true when a single probe has no bloom block with
    //   matching bits
    let blocked_matches = query!(
        r#"blocked_matches AS (
            SELECT
                cd.cp_block_index,
                cd.cp_block_index * {BigInt} as cp_lo,
                cd.cp_block_index * {BigInt} + {BigInt} - 1 as cp_hi
            FROM (SELECT DISTINCT cp_block_index FROM condition_data) cd
            WHERE NOT EXISTS (
                SELECT 1 FROM condition_data c
                WHERE c.cp_block_index = cd.cp_block_index
                  AND NOT EXISTS (
                      SELECT 1 FROM cp_bloom_blocks bb
                      WHERE bb.cp_block_index = c.cp_block_index
                        AND bb.bloom_block_index = c.bloom_idx
                        AND bloom_contains(bb.bloom_filter, c.byte_pos, c.bit_masks)
                  )
            )
            ORDER BY cp_lo {}
            LIMIT {BigInt}
        )"#,
        CP_BLOCK_SIZE as i64,
        CP_BLOCK_SIZE as i64,
        CP_BLOCK_SIZE as i64,
        page.order_by_direction(),
        fpr_adjusted_limit,
    );

    // Expand matched blocks into individual checkpoint sequences.
    let candidate_cps = query!(
        r#"candidate_cps AS (
            SELECT DISTINCT gs.cp AS cp_sequence_number
            FROM blocked_matches bm
            CROSS JOIN LATERAL generate_series(
                GREATEST(bm.cp_lo, {BigInt}),
                LEAST(bm.cp_hi, {BigInt})
            ) AS gs(cp)
        )"#,
        cp_lo as i64,
        cp_hi_inclusive as i64,
    );

    // Check each candidate checkpoint's bloom filter.
    let query = query!(
        r#"
        WITH {}
        , {}
        , {}
        SELECT cb.cp_sequence_number::BIGINT
        FROM cp_blooms cb
        JOIN candidate_cps cc ON cb.cp_sequence_number = cc.cp_sequence_number
        WHERE {}
        ORDER BY cb.cp_sequence_number {}
        LIMIT {BigInt}
        "#,
        condition_data,
        blocked_matches,
        candidate_cps,
        cp_bloom_condition,
        page.order_by_direction(),
        fpr_adjusted_limit,
    );

    let results: Vec<CpResult> = conn
        .results(query)
        .await
        .context("Failed to execute bloom filter scan query")?;
    Ok(results
        .into_iter()
        .map(|r| r.cp_sequence_number as u64)
        .collect())
}

/// SQL VALUES clause specifying which block_index and bloom blocks to check and which bits must be set.
/// Uses parallel arrays of byte offsets and bit masks for efficient bloom_contains checks.
fn cp_bloom_block_probes_fragment(
    probes: impl Iterator<Item = (i64, BlockedBloomProbe)>,
) -> Query<'static> {
    let values = probes
        .map(
            |(
                cp_block_index,
                BlockedBloomProbe {
                    block_idx,
                    byte_offsets,
                    bit_masks,
                },
            )| {
                format!(
                    "({}::BIGINT, {}::SMALLINT, ARRAY[{}]::INT[], ARRAY[{}]::INT[])",
                    cp_block_index,
                    block_idx,
                    byte_offsets.iter().join(","),
                    bit_masks.iter().join(",")
                )
            },
        )
        .join(", ");
    query!("{}", Query::new(values))
}

/// SQL condition checking if all filter values are present in a checkpoint's bloom filter.
fn cp_bloom_condition_fragment(probe: &BloomProbe) -> Query<'static> {
    if probe.byte_offsets.is_empty() {
        return query!("TRUE");
    }

    let byte_offsets: Vec<i32> = probe.byte_offsets.iter().map(|&o| o as i32).collect();
    let bit_masks: Vec<i32> = probe.bit_masks.iter().map(|&m| m as i32).collect();
    query!(
        "bloom_contains(cb.bloom_filter, {Array<Integer>}, {Array<Integer>})",
        byte_offsets,
        bit_masks,
    )
}
