// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::pipeline::{concurrent::Handler, Processor};
use crate::schema::cp_mapping;
use anyhow::Result;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_pg_db::{self as db};
use sui_types::full_checkpoint_content::CheckpointData;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = cp_mapping)]
pub struct StoredCpMapping {
    pub cp_sequence_number: i64,
    pub tx_lo: i64,
    pub epoch: i64,
}

pub struct CpMapping;

impl Processor for CpMapping {
    const NAME: &'static str = "cp_mapping";

    type Value = StoredCpMapping;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number as i64;
        let network_total_transactions =
            checkpoint.checkpoint_summary.network_total_transactions as i64;
        let tx_lo = network_total_transactions - checkpoint.transactions.len() as i64;
        let epoch = checkpoint.checkpoint_summary.epoch as i64;
        Ok(vec![StoredCpMapping {
            cp_sequence_number,
            tx_lo,
            epoch,
        }])
    }
}

#[async_trait::async_trait]
impl Handler for CpMapping {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(cp_mapping::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
