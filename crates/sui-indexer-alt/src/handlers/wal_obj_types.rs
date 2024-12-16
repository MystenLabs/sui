// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{
    objects::{StoredObjectUpdate, StoredSumObjType, StoredWalObjType},
    schema::wal_obj_types,
};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

use super::sum_obj_types::SumObjTypes;

pub(crate) struct WalObjTypes;

impl Processor for WalObjTypes {
    const NAME: &'static str = "wal_obj_types";

    type Value = StoredObjectUpdate<StoredSumObjType>;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        SumObjTypes.process(checkpoint)
    }
}

#[async_trait::async_trait]
impl Handler for WalObjTypes {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        let values: Vec<_> = values
            .iter()
            .map(|value| StoredWalObjType {
                object_id: value.object_id.to_vec(),
                object_version: value.object_version as i64,

                owner_kind: value.update.as_ref().map(|o| o.owner_kind),
                owner_id: value.update.as_ref().and_then(|o| o.owner_id.clone()),

                package: value.update.as_ref().and_then(|o| o.package.clone()),
                module: value.update.as_ref().and_then(|o| o.module.clone()),
                name: value.update.as_ref().and_then(|o| o.name.clone()),
                instantiation: value.update.as_ref().and_then(|o| o.instantiation.clone()),

                cp_sequence_number: value.cp_sequence_number as i64,
            })
            .collect();

        Ok(diesel::insert_into(wal_obj_types::table)
            .values(&values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune(from: u64, to: u64, conn: &mut db::Connection<'_>) -> Result<usize> {
        let filter = wal_obj_types::table
            .filter(wal_obj_types::cp_sequence_number.between(from as i64, to as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}
