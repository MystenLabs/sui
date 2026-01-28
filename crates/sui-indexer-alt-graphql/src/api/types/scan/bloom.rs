// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Context;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
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
    filter_values: &[Vec<u8>],
    cp_lo: u64,
    cp_hi: u64,
    page: &Page<SCTransaction>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database for bloom filter scan")?;

    let cp_block_lo = cp_block_index(cp_lo);
    let cp_block_hi = cp_block_index(cp_hi);

    // Block index and probe for each block in the range. Seeds vary per block, so we must
    // construct probes for each block.
    let block_probes = (cp_block_lo..=cp_block_hi).flat_map(|id| {
        CpBlockedBloomFilter::probe(filter_values, id as u128)
            .into_iter()
            .map(move |probe| (id, probe))
    });

    let cp_bloom_blocks_condition = cp_bloom_block_probes_fragment(block_probes);
    let cp_bloom_condition = cp_bloom_condition_fragment(&CpBloomFilter::probe(filter_values));

    let fpr_adjusted_limit = (page.limit_with_overhead() as f64 * OVERFETCH_MULTIPLIER) as i64;
    let query = query!(
        r#"
        -- Inline table of probes: which bloom blocks to check and what bits must be set
        WITH condition_data(cp_block_index, bloom_idx, byte_pos, bit_masks) AS (VALUES {})

        -- Find a page of checkpoint blocks where all probes matched.
        , blocked_matches AS (
            SELECT
                bb.cp_block_index,
                bb.cp_block_index * {BigInt} as cp_lo,
                bb.cp_block_index * {BigInt} + {BigInt} - 1 as cp_hi
            FROM cp_bloom_blocks bb
            JOIN condition_data c ON bb.cp_block_index = c.cp_block_index
                                AND bb.bloom_block_index = c.bloom_idx
            WHERE bloom_contains(bb.bloom_filter, c.byte_pos, c.bit_masks)
            GROUP BY bb.cp_block_index
            HAVING COUNT(*) = (SELECT COUNT(*) FROM condition_data c2 WHERE c2.cp_block_index = bb.cp_block_index)
             -- ^ Only keep blocks where ALL probes matched
            ORDER BY cp_lo {}
            LIMIT {BigInt}
            -- ^ Limit number of matching blocks to a page of results
        )

        -- Expand matched blocks into individual checkpoint sequences
        , candidate_cps AS (
            SELECT DISTINCT gs.cp AS cp_sequence_number
            FROM blocked_matches bm
            CROSS JOIN LATERAL generate_series(
                GREATEST(bm.cp_lo, {BigInt}),
                LEAST(bm.cp_hi, {BigInt})
            ) AS gs(cp)
        )

        -- Check each candidate checkpoint's bloom filter
        SELECT cb.cp_sequence_number::BIGINT
        FROM cp_blooms cb
        JOIN candidate_cps cc ON cb.cp_sequence_number = cc.cp_sequence_number
        WHERE {}
        ORDER BY cb.cp_sequence_number {}
        LIMIT {BigInt}
        "#,
        cp_bloom_blocks_condition,
        CP_BLOCK_SIZE as i64,
        CP_BLOCK_SIZE as i64,
        CP_BLOCK_SIZE as i64,
        page.order_by_direction(),
        fpr_adjusted_limit,
        cp_lo as i64,
        cp_hi as i64,
        cp_bloom_condition,
        page.order_by_direction(),
        fpr_adjusted_limit
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

    let condition = format!(
        "bloom_contains(cb.bloom_filter, ARRAY[{}]::INT[], ARRAY[{}]::INT[])",
        probe.byte_offsets.iter().join(","),
        probe.bit_masks.iter().join(",")
    );
    query!("{}", Query::new(condition))
}
