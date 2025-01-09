// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, ops::Range, sync::Arc};

use anyhow::{Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::tx_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{events::StoredEvStructInst, schema::ev_struct_inst};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

pub(crate) struct EvStructInst;

impl Processor for EvStructInst {
    const NAME: &'static str = "ev_struct_inst";

    type Value = StoredEvStructInst;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = BTreeSet::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            for (j, ev) in tx.events.iter().flat_map(|evs| evs.data.iter().enumerate()) {
                values.insert(StoredEvStructInst {
                    package: ev.type_.address.to_vec(),
                    module: ev.type_.module.to_string(),
                    name: ev.type_.name.to_string(),
                    instantiation: bcs::to_bytes(&ev.type_.type_params)
                        .with_context(|| format!(
                            "Failed to serialize type parameters for event ({tx_sequence_number}, {j})"
                        ))?,
                    tx_sequence_number: (first_tx + i) as i64,
                    sender: ev.sender.to_vec(),
                });
            }
        }

        Ok(values.into_iter().collect())
    }
}

#[async_trait::async_trait]
impl Handler for EvStructInst {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(ev_struct_inst::table)
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

        let filter = ev_struct_inst::table
            .filter(ev_struct_inst::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}
