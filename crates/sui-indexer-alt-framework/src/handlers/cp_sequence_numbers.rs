// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::pipeline::{concurrent::Handler, Processor};
use crate::schema::cp_sequence_numbers;
use anyhow::Result;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_pg_db::{self as db, Connection};
use sui_types::full_checkpoint_content::CheckpointData;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = cp_sequence_numbers)]
pub struct StoredCpSequenceNumbers {
    pub cp_sequence_number: i64,
    pub tx_lo: i64,
    pub epoch: i64,
}

<<<<<<< HEAD:crates/sui-indexer-alt-framework/src/handlers/cp_sequence_numbers.rs
pub struct CpSequenceNumbers;

=======
/// A struct that can be instantiated by the pruner task to map a `from` and `to` checkpoint to its
/// corresponding `tx_lo` and containing epoch. The `from` checkpoint is expected to be inclusive,
/// and the `to` checkpoint is exclusive. This requires the existence of the `checkpoint_metadata`
/// table.
>>>>>>> 0ad4e6f393 (address first set of comments. trait fn prune still accepts from and to checkpoints, up to handler prune to decide what it needs. doc comments for PrunableRange regarding what it expects on instantiation):crates/sui-indexer-alt-framework/src/handlers/cp_mapping.rs
pub struct PrunableRange {
    from: StoredCpMapping,
    to: StoredCpMapping,
}

impl PrunableRange {
    /// Gets the tx and epoch mappings for both the start and end checkpoints.
    ///
    /// The values are expected to exist since the cp_mapping table must have enough information to
    /// encompass the retention of other tables.
    pub async fn get_range(
        conn: &mut Connection<'_>,
        from_cp: u64,
        to_cp: u64,
    ) -> QueryResult<Self> {
        let results = cp_mapping::table
            .select(StoredCpMapping::as_select())
            .filter(cp_mapping::cp_sequence_number.eq_any([from_cp as i64, to_cp as i64]))
            .order(cp_mapping::cp_sequence_number.asc())
            .load::<StoredCpMapping>(conn)
            .await?;

        match results.as_slice() {
            [first, .., last] => Ok(PrunableRange {
                from: first.clone(),
                to: last.clone(),
            }),
            _ => Err(diesel::result::Error::NotFound),
        }
    }

    /// Inclusive start and exclusive end range of prunable checkpoints.
    pub fn checkpoint_interval(&self) -> (u64, u64) {
        (
            self.from.cp_sequence_number as u64,
            self.to.cp_sequence_number as u64,
        )
    }

    /// Inclusive start and exclusive end range of prunable txs.
    pub fn tx_interval(&self) -> (u64, u64) {
        (self.from.tx_lo as u64, self.to.tx_lo as u64)
    }

    /// Inclusive start and exclusive end range of epochs.
    ///
    /// The two values in the tuple represent which epoch the `from` and `to` checkpoints come from,
    /// respectively.
    pub fn epoch_interval(&self) -> (u64, u64) {
        (self.from.epoch as u64, self.to.epoch as u64)
    }
}

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
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(cp_sequence_numbers::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
