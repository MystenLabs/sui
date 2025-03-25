// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::models::cp_sequence_numbers::StoredCpSequenceNumbers;

use crate::pg_store::PgStore;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_framework::schema::cp_sequence_numbers;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;

pub struct CpSequenceNumbers;

impl Processor for CpSequenceNumbers {
    const NAME: &'static str = "cp_sequence_numbers";

    type Value = StoredCpSequenceNumbers;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number as i64;
        let network_total_transactions =
            checkpoint.checkpoint_summary.network_total_transactions as i64;
        let tx_lo = network_total_transactions - checkpoint.transactions.len() as i64;
        let epoch = checkpoint.checkpoint_summary.epoch as i64;
        Ok(vec![StoredCpSequenceNumbers {
            cp_sequence_number,
            tx_lo,
            epoch,
        }])
    }
}

#[async_trait::async_trait]
impl Handler for CpSequenceNumbers {
    type Store = PgStore;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        Ok(diesel::insert_into(cp_sequence_numbers::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
