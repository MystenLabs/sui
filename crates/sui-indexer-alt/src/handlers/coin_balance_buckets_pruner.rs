// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::ExpressionMethods;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::schema::coin_balance_buckets;
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
        let mut committed_rows = 0;
        for (object_id, cp_sequence_number_exclusive) in to_prune {
            committed_rows += diesel::delete(coin_balance_buckets::table)
                .filter(coin_balance_buckets::object_id.eq(object_id.as_slice()))
                .filter(coin_balance_buckets::cp_sequence_number.lt(cp_sequence_number_exclusive))
                .execute(conn)
                .await?;
        }
        Ok(committed_rows)
    }
}
