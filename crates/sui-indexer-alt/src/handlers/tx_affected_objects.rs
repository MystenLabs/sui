// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::Result;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::tx_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{schema::tx_affected_objects, transactions::StoredTxAffectedObject};
use sui_pg_db as db;
use sui_types::{effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData};

pub(crate) struct TxAffectedObjects;

impl Processor for TxAffectedObjects {
    const NAME: &'static str = "tx_affected_objects";

    type Value = StoredTxAffectedObject;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = Vec::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            let sender = tx.transaction.sender_address();

            values.extend(
                tx.effects
                    .object_changes()
                    .iter()
                    .map(|o| StoredTxAffectedObject {
                        tx_sequence_number,
                        affected: o.id.to_vec(),
                        sender: sender.to_vec(),
                    }),
            );
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for TxAffectedObjects {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_affected_objects::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> Result<usize> {
        let Range {
            start: from_tx,
            end: to_tx,
        } = tx_interval(conn, from..to_exclusive).await?;
        let filter = tx_affected_objects::table.filter(
            tx_affected_objects::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1),
        );

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}
