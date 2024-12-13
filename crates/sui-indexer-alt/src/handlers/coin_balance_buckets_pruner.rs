// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

use super::coin_balance_buckets::{
    CoinBalanceBucketChangeKind, CoinBalanceBuckets, ProcessedCoinBalanceBucket,
};

pub(crate) struct CoinBalanceBucketsPruner;

impl Processor for CoinBalanceBucketsPruner {
    const NAME: &'static str = "coin_balance_buckets_pruner";
    type Value = ProcessedCoinBalanceBucket;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        CoinBalanceBuckets.process(checkpoint)
    }
}

#[async_trait::async_trait]
impl Handler for CoinBalanceBucketsPruner {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        use sui_indexer_alt_schema::schema::coin_balance_buckets::dsl;

        let mut to_prune = BTreeMap::new();
        for v in values {
            let object_id = v.object_id;
            let cp_sequence_number_exclusive = match v.change {
                CoinBalanceBucketChangeKind::Insert { .. } => v.cp_sequence_number,
                CoinBalanceBucketChangeKind::Delete => v.cp_sequence_number + 1,
            } as i64;
            let cp = to_prune.entry(object_id).or_default();
            *cp = std::cmp::max(*cp, cp_sequence_number_exclusive);
        }
        let values = to_prune
            .iter()
            .map(|(object_id, seq_number)| {
                let object_id_hex = hex::encode(object_id);
                format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, seq_number)
            })
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "
            WITH to_prune_data (object_id, cp_sequence_number_exclusive) AS (
                VALUES {}
            )
            DELETE FROM coin_balance_buckets
            USING to_prune_data
            WHERE coin_balance_buckets.{:?} = to_prune_data.object_id
              AND coin_balance_buckets.{:?} < to_prune_data.cp_sequence_number_exclusive
            ",
            values,
            dsl::object_id,
            dsl::cp_sequence_number,
        );
        let rows_deleted = sql_query(query).execute(conn).await?;
        Ok(rows_deleted)
    }
}
