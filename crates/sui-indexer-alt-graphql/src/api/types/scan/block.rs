// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Reverse;
use std::collections::HashMap;

use async_graphql::Context;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::blocked::BlockedBloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::BLOOM_BLOCK_BITS;
use sui_indexer_alt_schema::cp_bloom_blocks::NUM_BLOOM_BLOCKS;
use sui_indexer_alt_schema::cp_bloom_blocks::NUM_HASHES;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_id;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_seed;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::types::transaction::SCTransaction;
use crate::error::RpcError;
use crate::pagination::Page;

/// A single bloom filter condition: which checkpoint block, which bloom block index,
/// and the byte positions + bit masks to check.
struct BloomCondition {
    cp_block_id: i64,
    bloom_block_idx: i16,
    byte_positions: Vec<usize>,
    bit_masks: Vec<usize>,
}

#[derive(diesel::QueryableByName, Debug)]
struct CpBlockRange {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    cp_lo: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    cp_hi: i64,
}

pub(crate) async fn query_blocked_blooms(
    ctx: &Context<'_>,
    filter_keys: &[Vec<u8>],
    cp_lo: u64,
    cp_hi: u64,
    page: &Page<SCTransaction>,
) -> Result<Vec<u64>, RpcError> {
    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader.connect().await?;

    let cp_block_lo = cp_block_id(cp_lo);
    let cp_block_hi = cp_block_id(cp_hi);

    let (conditions, required_counts) =
        compute_bloom_conditions(filter_keys, cp_block_lo, cp_block_hi);

    let condition_values = build_condition_values(&conditions);
    let req_counts_values = build_req_counts_values(&required_counts);
    let limit = page.limit_with_overhead() as i64;

    let query = query!(
        r#"
WITH required_counts(cp_block_id, req_count) AS (VALUES {})
, condition_data(cp_block_id, bloom_idx, byte_pos, bit_masks) AS (VALUES {})
SELECT
    MIN(bb.cp_sequence_number_lo)::BIGINT as cp_lo,
    MAX(bb.cp_sequence_number_hi)::BIGINT as cp_hi
FROM cp_bloom_blocks bb
JOIN condition_data c ON bb.cp_block_id = c.cp_block_id
                     AND bb.bloom_block_index = c.bloom_idx
JOIN required_counts rc ON bb.cp_block_id = rc.cp_block_id
WHERE check_bloom_bits(bb.bloom_filter, c.byte_pos, c.bit_masks)
GROUP BY bb.cp_block_id, rc.req_count
HAVING COUNT(DISTINCT c.bloom_idx)::BIGINT >= rc.req_count
ORDER BY cp_lo {}
LIMIT {BigInt}
"#,
        Query::new(&req_counts_values),
        Query::new(&condition_values),
        page.order_by_direction(),
        limit
    );

    let candidate_ranges: Vec<CpBlockRange> = conn.results(query).await?;

    Ok(expand_ranges(
        &candidate_ranges,
        cp_lo,
        cp_hi,
        page.is_from_front(),
    ))
}

/// Compute bloom filter conditions for all checkpoint blocks in range.
/// Returns:
/// - A list of BloomCondition (one per cp_block_id + filter_key combination)
/// - A map of cp_block_id -> required bloom block count
fn compute_bloom_conditions(
    filter_keys: &[Vec<u8>],
    cp_block_lo: i64,
    cp_block_hi: i64,
) -> (Vec<BloomCondition>, HashMap<i64, usize>) {
    let mut conditions = Vec::new();
    let mut required_counts: HashMap<i64, usize> = HashMap::new();

    for cp_block_id in cp_block_lo..=cp_block_hi {
        let seed = cp_block_seed(cp_block_id);
        let mut bloom_blocks_for_cp: std::collections::HashSet<i16> =
            std::collections::HashSet::new();

        for key in filter_keys {
            let (block_idx, positions) = BlockedBloomFilter::hash(
                key,
                seed,
                NUM_BLOOM_BLOCKS,
                NUM_HASHES,
                BLOOM_BLOCK_BITS,
            );

            bloom_blocks_for_cp.insert(block_idx as i16);

            // Convert bit positions to byte positions and bit masks
            let byte_positions: Vec<usize> = positions.iter().map(|p| p / 8).collect();
            let bit_masks: Vec<usize> = positions.iter().map(|p| 1 << (p % 8)).collect();

            conditions.push(BloomCondition {
                cp_block_id,
                bloom_block_idx: block_idx as i16,
                byte_positions,
                bit_masks,
            });
        }

        required_counts.insert(cp_block_id, bloom_blocks_for_cp.len());
    }

    (conditions, required_counts)
}

fn expand_ranges(ranges: &[CpBlockRange], cp_lo: u64, cp_hi: u64, ascending: bool) -> Vec<u64> {
    let mut candidates: Vec<u64> = ranges
        .iter()
        .flat_map(|range| {
            let range_lo = (range.cp_lo as u64).max(cp_lo);
            let range_hi = (range.cp_hi as u64).min(cp_hi);
            range_lo..=range_hi
        })
        .collect();

    if ascending {
        candidates.sort_unstable();
    } else {
        candidates.sort_unstable_by_key(|&x| Reverse(x));
    }
    candidates
}

fn build_condition_values(conditions: &[BloomCondition]) -> String {
    conditions
        .iter()
        .map(|c| {
            format!(
                "({}::BIGINT, {}::SMALLINT, ARRAY[{}], ARRAY[{}])",
                c.cp_block_id,
                c.bloom_block_idx,
                c.byte_positions
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                c.bit_masks
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n        ")
}

fn build_req_counts_values(required_counts: &HashMap<i64, usize>) -> String {
    required_counts
        .iter()
        .map(|(cp_id, count)| format!("({}::BIGINT, {}::BIGINT)", cp_id, count))
        .collect::<Vec<_>>()
        .join(", ")
}
