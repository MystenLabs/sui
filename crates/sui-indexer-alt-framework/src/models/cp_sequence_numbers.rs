// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::cp_sequence_numbers;
use anyhow::{bail, Result};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::ops::Range;
use sui_field_count::FieldCount;
use sui_pg_db::Connection;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = cp_sequence_numbers)]
pub struct StoredCpSequenceNumbers {
    pub cp_sequence_number: i64,
    pub tx_lo: i64,
    pub epoch: i64,
}

/// A struct that can be instantiated by the pruner task to map a `from` and `to` checkpoint to its
/// corresponding `tx_lo` and containing epoch. The `from` checkpoint is expected to be inclusive,
/// and the `to` checkpoint is exclusive. This requires the existence of the `checkpoint_metadata`
/// table.
pub struct PrunableRange {
    from: StoredCpSequenceNumbers,
    to: StoredCpSequenceNumbers,
}

impl PrunableRange {
    /// Gets the tx and epoch mappings for both the start and end checkpoints.
    ///
    /// The values are expected to exist since the cp_mapping table must have enough information to
    /// encompass the retention of other tables.
    pub async fn get_range(conn: &mut Connection<'_>, cps: Range<u64>) -> Result<Self> {
        let Range {
            start: from_cp,
            end: to_cp,
        } = cps;

        // Only error if from_cp is not <= to_cp. from_cp can be equal to to_cp, because there may
        // be multiple transactions within the same checkpoint.
        if from_cp > to_cp {
            bail!(format!(
                "Invalid checkpoint range: `from` {from_cp} is greater than `to` {to_cp}"
            ));
        }

        let results = cp_sequence_numbers::table
            .select(StoredCpSequenceNumbers::as_select())
            .filter(cp_sequence_numbers::cp_sequence_number.eq_any([from_cp as i64, to_cp as i64]))
            .order(cp_sequence_numbers::cp_sequence_number.asc())
            .load::<StoredCpSequenceNumbers>(conn)
            .await
            .map_err(anyhow::Error::from)?;

        let Some(from) = results
            .iter()
            .find(|cp| cp.cp_sequence_number == from_cp as i64)
        else {
            bail!(format!(
                "No checkpoint mapping found for checkpoint {from_cp}"
            ));
        };
        let Some(to) = results
            .iter()
            .find(|cp| cp.cp_sequence_number == to_cp as i64)
        else {
            bail!(format!(
                "No checkpoint mapping found for checkpoint {to_cp}"
            ));
        };

        Ok(PrunableRange {
            from: from.clone(),
            to: to.clone(),
        })
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
