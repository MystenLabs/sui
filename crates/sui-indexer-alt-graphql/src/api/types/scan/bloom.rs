// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Context;
use diesel::sql_types::BigInt;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::blooms::bloom::BloomFilter;
use sui_indexer_alt_schema::cp_blooms::BLOOM_FILTER_SEED;
use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_BITS;
use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_HASHES;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

use crate::api::types::transaction::SCTransaction;
use crate::error::RpcError;
use crate::pagination::Page;

use super::bit_checks;

const CHECKPOINT_BATCH_SIZE: usize = 5000;
const CHECKPOINT_SCAN_LIMIT: usize = 200;

#[derive(diesel::QueryableByName, Debug)]
struct CpResult {
    #[diesel(sql_type = BigInt)]
    cp_sequence_number: i64,
}

pub(crate) async fn candidate_cp_blooms(
    ctx: &Context<'_>,
    filter_keys: &[Vec<u8>],
    candidates: &[u64],
    page: &Page<SCTransaction>,
) -> Result<Vec<u64>, RpcError> {
    if candidates.is_empty() || filter_keys.is_empty() {
        return Ok(candidates.to_vec());
    }

    let pg_reader: &PgReader = ctx.data()?;
    let mut conn = pg_reader.connect().await?;

    let condition = bloom_condition(filter_keys);
    let mut filtered_checkpoints = Vec::new();

    for chunk in candidates.chunks(CHECKPOINT_BATCH_SIZE) {
        let remaining_limit = CHECKPOINT_SCAN_LIMIT.saturating_sub(filtered_checkpoints.len());
        let cp_array: Vec<i64> = chunk.iter().map(|&cp| cp as i64).collect();

        let results: Vec<CpResult> = {
            let q = query!(
                r#"
                SELECT cp_sequence_number::BIGINT
                FROM cp_blooms
                WHERE cp_sequence_number = ANY({Array<BigInt>})
                  AND {}
                ORDER BY cp_sequence_number {}
                LIMIT {BigInt}
                "#,
                cp_array,
                Query::new(&condition),
                page.order_by_direction(),
                remaining_limit as i64
            );
            conn.results(q).await?
        };
        filtered_checkpoints.extend(results.into_iter().map(|r| r.cp_sequence_number as u64));

        if filtered_checkpoints.len() >= CHECKPOINT_SCAN_LIMIT {
            break;
        }
    }

    Ok(filtered_checkpoints)
}

fn bloom_condition(filter_keys: &[Vec<u8>]) -> String {
    let keys_positions: Vec<Vec<usize>> = filter_keys
        .iter()
        .map(|key| {
            BloomFilter::hash(
                key,
                BLOOM_FILTER_SEED,
                CP_BLOOM_NUM_BITS,
                CP_BLOOM_NUM_HASHES,
            )
        })
        .collect();
    bit_checks(&keys_positions, "bloom_filter", None)
}
