// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::{Result, bail};
use async_trait::async_trait;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{Processor, concurrent::Handler},
    postgres::{Connection, Db},
    types::full_checkpoint_content::CheckpointData,
};
use sui_indexer_alt_schema::cp_sequence_numbers::StoredCpSequenceNumbers;
use sui_indexer_alt_schema::schema::cp_sequence_numbers;

pub struct CpSequenceNumbers;

#[async_trait]
impl Processor for CpSequenceNumbers {
    const NAME: &'static str = "cp_sequence_numbers";

    type Value = StoredCpSequenceNumbers;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
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

#[async_trait]
impl Handler for CpSequenceNumbers {
    type Store = Db;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(cp_sequence_numbers::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
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
