// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use async_trait::async_trait;
use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};

use crate::{
    backfill::backfill_task::BackfillTask,
    database::ConnectionPool,
    models::tx_indices::StoredTxAffectedObjects,
    schema::{transactions, tx_affected_objects, tx_senders},
};

pub struct TxAffectedObjectsBackfill;

#[async_trait]
impl BackfillTask for TxAffectedObjectsBackfill {
    async fn backfill_range(&self, pool: ConnectionPool, range: &RangeInclusive<usize>) {
        use transactions::dsl as tx;
        use tx_senders::dsl as ts;

        let mut conn = pool.get().await.unwrap();

        let join = tx_senders::table.on(tx::tx_sequence_number.eq(ts::tx_sequence_number));

        let results: Vec<(i64, Vec<u8>, Vec<u8>)> = transactions::table
            .inner_join(join)
            .select((tx::tx_sequence_number, ts::sender, tx::raw_effects))
            .filter(tx::tx_sequence_number.between(*range.start() as i64, *range.end() as i64))
            .load(&mut conn)
            .await
            .unwrap();

        let effects: Vec<(i64, Vec<u8>, TransactionEffects)> = results
            .into_iter()
            .map(|(tx_sequence_number, sender, bytes)| {
                (tx_sequence_number, sender, bcs::from_bytes(&bytes).unwrap())
            })
            .collect();

        let affected_objects: Vec<StoredTxAffectedObjects> = effects
            .into_iter()
            .flat_map(|(tx_sequence_number, sender, effects)| {
                effects
                    .object_changes()
                    .into_iter()
                    .map(move |change| StoredTxAffectedObjects {
                        tx_sequence_number,
                        affected: change.id.to_vec(),
                        sender: sender.clone(),
                    })
            })
            .collect();

        for chunk in affected_objects.chunks(1000) {
            diesel::insert_into(tx_affected_objects::table)
                .values(chunk)
                .on_conflict_do_nothing()
                .execute(&mut conn)
                .await
                .unwrap();
        }
    }
}
