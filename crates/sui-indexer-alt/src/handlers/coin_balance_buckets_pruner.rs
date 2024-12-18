// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_pg_db as db;
use sui_types::{base_types::ObjectID, full_checkpoint_content::CheckpointData};

use super::coin_balance_buckets::{get_coin_balance_bucket, get_coin_owner};

pub(crate) struct CoinBalanceBucketsPruner;

pub(crate) struct CoinBalanceBucketToBePruned {
    pub object_id: ObjectID,
    pub cp_sequence_number_exclusive: u64,
}

impl Processor for CoinBalanceBucketsPruner {
    const NAME: &'static str = "coin_balance_buckets_pruner";
    type Value = CoinBalanceBucketToBePruned;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint.checkpoint_input_objects();
        let latest_live_output_objects: BTreeMap<_, _> = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect();
        let mut values = Vec::new();
        for (object_id, input_object) in checkpoint_input_objects {
            // This loop processes all coins that were owned by a single address prior to the checkpoint,
            // but is now deleted/wrapped, or changed owner or coin balance bucket the checkpoint.
            if !input_object.is_coin() {
                continue;
            }
            let Some(input_coin_owner) = get_coin_owner(input_object) else {
                continue;
            };
            let input_coin_balance_bucket = get_coin_balance_bucket(input_object)?;
            if let Some(output_object) = latest_live_output_objects.get(&object_id) {
                let output_coin_owner = get_coin_owner(output_object);
                let output_coin_balance_bucket = get_coin_balance_bucket(output_object)?;
                if (output_coin_owner, output_coin_balance_bucket)
                    != (Some(input_coin_owner), input_coin_balance_bucket)
                {
                    values.push(CoinBalanceBucketToBePruned {
                        object_id,
                        cp_sequence_number_exclusive: cp_sequence_number,
                    });
                }
            } else {
                values.push(CoinBalanceBucketToBePruned {
                    object_id,
                    cp_sequence_number_exclusive: cp_sequence_number + 1,
                });
            }
        }
        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for CoinBalanceBucketsPruner {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        use sui_indexer_alt_schema::schema::coin_balance_buckets::dsl;

        let mut to_prune = BTreeMap::new();
        for v in values {
            let cp = to_prune.entry(v.object_id).or_default();
            *cp = std::cmp::max(*cp, v.cp_sequence_number_exclusive);
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

impl FieldCount for CoinBalanceBucketToBePruned {
    // This does not really matter since we are not limited by postgres' bound variable limit, because
    // we don't bind parameters in the deletion statement.
    const FIELD_COUNT: usize = 1;
}
