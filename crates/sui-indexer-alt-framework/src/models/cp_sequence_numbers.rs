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

/// Inclusive start and exclusive end range of prunable txs.
pub async fn tx_interval(conn: &mut Connection<'_>, cps: Range<u64>) -> Result<Range<u64>> {
    let result = get_range(conn, cps).await?;

    Ok(Range {
        start: result.0.tx_lo as u64,
        end: result.1.tx_lo as u64,
    })
}

/// Returns the epochs of the given checkpoint range. `start` is the epoch of the first checkpoint
/// and `end` is the epoch of the last checkpoint.
pub async fn epoch_interval(conn: &mut Connection<'_>, cps: Range<u64>) -> Result<Range<u64>> {
    let result = get_range(conn, cps).await?;

    Ok(Range {
        start: result.0.epoch as u64,
        end: result.1.epoch as u64,
    })
}

/// Gets the tx and epoch mappings for the given checkpoint range.
///
/// The values are expected to exist since the cp_sequence_numbers table must have enough information to
/// encompass the retention of other tables.
pub(crate) async fn get_range(
    conn: &mut Connection<'_>,
    cps: Range<u64>,
) -> Result<(StoredCpSequenceNumbers, StoredCpSequenceNumbers)> {
    let Range {
        start: from_cp,
        end: to_cp,
    } = cps;

    if from_cp >= to_cp {
        bail!(format!(
            "Invalid checkpoint range: `from` {from_cp} must be less than `to` {to_cp}"
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

    Ok((from.clone(), to.clone()))
}
