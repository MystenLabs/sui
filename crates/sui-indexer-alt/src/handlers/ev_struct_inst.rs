// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{db, models::events::StoredEvStructInst, schema::ev_struct_inst};

use super::Handler;

pub struct EvStructInst;

#[async_trait::async_trait]
impl Handler for EvStructInst {
    const NAME: &'static str = "ev_struct_inst";

    const BATCH_SIZE: usize = 100;
    const CHUNK_SIZE: usize = 1000;
    const MAX_PENDING_SIZE: usize = 10000;

    type Value = StoredEvStructInst;

    fn handle(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
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

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(ev_struct_inst::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
