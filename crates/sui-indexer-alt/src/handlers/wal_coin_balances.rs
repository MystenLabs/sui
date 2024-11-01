// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{
    db,
    models::objects::{StoredObjectUpdate, StoredSumCoinBalance, StoredWalCoinBalance},
    pipeline::{concurrent::Handler, Processor},
    schema::wal_coin_balances,
};

use super::sum_coin_balances::SumCoinBalances;

pub struct WalCoinBalances;

impl Processor for WalCoinBalances {
    const NAME: &'static str = "wal_coin_balances";

    type Value = StoredObjectUpdate<StoredSumCoinBalance>;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        SumCoinBalances::process(checkpoint)
    }
}

#[async_trait::async_trait]
impl Handler for WalCoinBalances {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_CHUNK_ROWS: usize = 1000;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        let values: Vec<_> = values
            .iter()
            .map(|value| StoredWalCoinBalance {
                object_id: value.object_id.to_vec(),
                object_version: value.object_version as i64,

                owner_id: value.update.as_ref().map(|o| o.owner_id.clone()),

                coin_type: value.update.as_ref().map(|o| o.coin_type.clone()),
                coin_balance: value.update.as_ref().map(|o| o.coin_balance),

                cp_sequence_number: value.cp_sequence_number as i64,
            })
            .collect();

        Ok(diesel::insert_into(wal_coin_balances::table)
            .values(&values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
