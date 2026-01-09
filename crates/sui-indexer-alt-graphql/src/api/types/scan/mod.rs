// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod block;
mod bloom;

pub(crate) use block::query_blocked_blooms;
pub(crate) use bloom::candidate_cp_blooms;

use diesel::sql_types::Integer;
use sui_pg_db::query::Query;
use sui_sql_macro::query;

/// Bloom filter "contains()" check query fragment. Used to check that the bloom filter has
/// bits set at the given indexes.
pub(super) fn contains_fragment<T: AsRef<[usize]>>(keys_positions: &[T]) -> Query<'static> {
    keys_positions
        .iter()
        .map(|positions| {
            let positions = positions.as_ref();
            let byte_positions: Vec<i32> = positions.iter().map(|p| (p / 8) as i32).collect();
            let bit_masks: Vec<i32> = positions.iter().map(|p| 1 << (p % 8)).collect();

            query!(
                "bloom_contains(bloom_filter, {Array<Integer>}, {Array<Integer>})",
                byte_positions,
                bit_masks
            )
        })
        .reduce(|a, b| a + query!(" AND ") + b)
        .unwrap_or_else(|| query!("TRUE"))
}
