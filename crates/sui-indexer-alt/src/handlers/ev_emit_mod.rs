// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{db, models::events::StoredEvEmitMod, schema::ev_emit_mod};

use super::Handler;

pub struct EvEmitMod;

#[async_trait::async_trait]
impl Handler for EvEmitMod {
    const NAME: &'static str = "ev_emit_mod";

    const BATCH_SIZE: usize = 100;
    const CHUNK_SIZE: usize = 1000;
    const MAX_PENDING_SIZE: usize = 10000;

    type Value = StoredEvEmitMod;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = BTreeSet::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            values.extend(
                tx.events
                    .iter()
                    .flat_map(|evs| &evs.data)
                    .map(|ev| StoredEvEmitMod {
                        package: ev.package_id.to_vec(),
                        module: ev.transaction_module.to_string(),
                        tx_sequence_number: (first_tx + i) as i64,
                        sender: ev.sender.to_vec(),
                    }),
            );
        }

        Ok(values.into_iter().collect())
    }

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(ev_emit_mod::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
