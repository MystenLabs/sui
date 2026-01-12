// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context as _;
use async_graphql::Context;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::blocked::BlockedBloomFilter;
use sui_indexer_alt_schema::blooms::bloom::BloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::BLOOM_BLOCK_BITS;
use sui_indexer_alt_schema::cp_bloom_blocks::NUM_BLOOM_BLOCKS;
use sui_indexer_alt_schema::cp_bloom_blocks::NUM_HASHES;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_id;
use sui_indexer_alt_schema::cp_blooms::BLOOM_FILTER_SEED;
use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_BITS;
use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_HASHES;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::error::RpcError;
use crate::pagination::Page;

const OVERFETCH_MULTIPLIER: usize = 2;

/// Probe that represent the block and bits that should be set for values if they exists in a checkpoint block.
/// We pre-compute these probes to avoid repeated hashing in SQL.
struct CpBloomBlockProbe {
    // Checkpoint block ID to check for a value
    cp_block_id: i64,
    // The index of the bloom block the value hashes to
    bloom_block_idx: i16,
    // Array of byte offsets within the bloom block where bits are set
    byte_offsets: Vec<usize>,
    // Array of set bits corresponding to the byte offsets
    bit_masks: Vec<usize>,
}

/// Probe that represent the bits that should be set for values if they exist in a checkpoint.
struct CpBloomProbe {
    byte_offsets: Vec<usize>,
    bit_masks: Vec<usize>,
}

#[derive(QueryableByName)]
struct CpResult {
    #[diesel(sql_type = BigInt)]
    cp_sequence_number: i64,
}

/// The checkpoints that might contain the filter criteria.
///
/// Does a coarse filter over checkpoints ranges using cp_bloom_blocks,
/// then a finer filter over those ranges for checkpoint matches using cp_blooms.
pub(crate) async fn candidate_cps<C>(
    ctx: &Context<'_>,
    filter_values: &[Vec<u8>],
    cp_lo: u64,
    cp_hi: u64,
    page: &Page<C>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database for bloom filter scan")?;

    let cp_block_lo = cp_block_id(cp_lo);
    let cp_block_hi = cp_block_id(cp_hi);

    // Phase 1: probes for blocked bloom filters (varying seeds per block)
    let block_probes = cp_bloom_block_probes(filter_values, cp_block_lo, cp_block_hi);
    let cp_bloom_blocks_condition = cp_bloom_block_probes_fragment(&block_probes);

    // Phase 2: probe for per-checkpoint bloom filters (fixed seed)
    let cp_bloom_probe = cp_bloom_probe(filter_values);
    let cp_bloom_condition = cp_bloom_condition_fragment(&cp_bloom_probe);

    let query = query!(
        r#"
        -- Inline table of probes: which bloom blocks to check and what bits must be set
        WITH condition_data(cp_block_id, bloom_idx, byte_pos, bit_masks) AS (VALUES {})

        -- Find a page of checkpoint blocks where all probes matched.
        , blocked_matches AS (
            SELECT
                bb.cp_block_id,
                MIN(bb.cp_sequence_number_lo) as cp_lo,
                MAX(bb.cp_sequence_number_hi) as cp_hi
            FROM cp_bloom_blocks bb
            JOIN condition_data c ON bb.cp_block_id = c.cp_block_id
                                AND bb.bloom_block_index = c.bloom_idx
            WHERE bloom_contains(bb.bloom_filter, c.byte_pos, c.bit_masks)
            GROUP BY bb.cp_block_id
            HAVING COUNT(*) = (SELECT COUNT(*) FROM condition_data c2 WHERE c2.cp_block_id = bb.cp_block_id)
             -- ^ Only keep blocks where ALL probes matched (AND semantics)
            ORDER BY cp_lo {}
            LIMIT {BigInt}
            -- ^ Limit number of matching blocks to a page of results
        )

        -- Expand matchedd blocks into individual checkpoint sequences
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
        page.order_by_direction(),
        (page.limit_with_overhead() * OVERFETCH_MULTIPLIER) as i64,
        cp_lo as i64,
        cp_hi as i64,
        cp_bloom_condition,
        page.order_by_direction(),
        (page.limit_with_overhead() * OVERFETCH_MULTIPLIER) as i64
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

/// Bloom filter probes for checkpoint blocks in range. Used to find the block and bits
/// that should be set if the filter keys are present in that checkpoint block.
///
/// Each probe contains parallel arrays of byte_offsets and bit_masks. These are passed to
/// the SQL `bloom_contains` function which checks: for each (byte_offset, bit_mask) pair,
/// is `(bloom_filter[byte_offset] & bit_mask) == bit_mask`? All pairs must pass.
fn cp_bloom_block_probes(
    filter_values: &[Vec<u8>],
    cp_block_lo: i64,
    cp_block_hi: i64,
) -> Vec<CpBloomBlockProbe> {
    let mut probes = Vec::new();

    for cp_block_id in cp_block_lo..=cp_block_hi {
        let seed = cp_block_id as u128;
        let mut by_block: HashMap<i16, CpBloomBlockProbe> = HashMap::new();

        for key in filter_values {
            let (idx, bits) =
                BlockedBloomFilter::hash(key, seed, NUM_BLOOM_BLOCKS, NUM_HASHES, BLOOM_BLOCK_BITS);

            let probe = by_block
                .entry(idx as i16)
                .or_insert_with(|| CpBloomBlockProbe {
                    cp_block_id,
                    bloom_block_idx: idx as i16,
                    byte_offsets: Vec::new(),
                    bit_masks: Vec::new(),
                });

            for b in &bits {
                probe.byte_offsets.push(b / 8);
                probe.bit_masks.push(1 << (b % 8));
            }
        }

        probes.extend(by_block.into_values());
    }

    probes
}

/// The VALUES fragment for the cp_bloom_blocks probes.
fn cp_bloom_block_probes_fragment(probes: &[CpBloomBlockProbe]) -> Query<'static> {
    let values = probes
        .iter()
        .map(|p| {
            format!(
                "({}::BIGINT, {}::SMALLINT, ARRAY[{}]::INT[], ARRAY[{}]::INT[])",
                p.cp_block_id,
                p.bloom_block_idx,
                p.byte_offsets
                    .iter()
                    .map(|o| o.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                p.bit_masks
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    query!("{}", Query::new(values))
}

/// Build a single probe combining all filter keys.
/// Since all keys must match (AND semantics), we just need all bits to be set.
fn cp_bloom_probe(filter_values: &[Vec<u8>]) -> CpBloomProbe {
    let mut probe = CpBloomProbe {
        byte_offsets: Vec::new(),
        bit_masks: Vec::new(),
    };

    for key in filter_values {
        let bits = BloomFilter::hash(
            key,
            BLOOM_FILTER_SEED,
            CP_BLOOM_NUM_BITS,
            CP_BLOOM_NUM_HASHES,
        );
        for b in &bits {
            probe.byte_offsets.push(b / 8);
            probe.bit_masks.push(1 << (b % 8));
        }
    }

    probe
}

/// The bloom_contains condition for per-checkpoint bloom filters.
fn cp_bloom_condition_fragment(probe: &CpBloomProbe) -> Query<'static> {
    if probe.byte_offsets.is_empty() {
        return query!("TRUE");
    }

    let condition = format!(
        "bloom_contains(cb.bloom_filter, ARRAY[{}]::INT[], ARRAY[{}]::INT[])",
        probe
            .byte_offsets
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>()
            .join(","),
        probe
            .bit_masks
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    query!("{}", Query::new(condition))
}
