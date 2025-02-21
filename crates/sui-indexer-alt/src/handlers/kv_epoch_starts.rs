// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::epoch_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{epochs::StoredEpochStart, schema::kv_epoch_starts};
use sui_pg_db as db;
use sui_types::{
    full_checkpoint_content::CheckpointData,
    sui_system_state::{get_sui_system_state, SuiSystemStateTrait},
    transaction::{TransactionDataAPI, TransactionKind},
};

pub(crate) struct KvEpochStarts;

impl Processor for KvEpochStarts {
    const NAME: &'static str = "kv_epoch_starts";

    type Value = StoredEpochStart;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            checkpoint_summary,
            transactions,
            ..
        } = checkpoint.as_ref();

        // If this is the last checkpoint in the current epoch, it will contain enough information
        // about the start of the next epoch.
        if !checkpoint_summary.is_last_checkpoint_of_epoch() {
            return Ok(vec![]);
        }

        let Some(transaction) = transactions.iter().find(|tx| {
            matches!(
                tx.transaction.intent_message().value.kind(),
                TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
            )
        }) else {
            bail!(
                "Failed to get end of epoch transaction in checkpoint {} with EndOfEpochData",
                checkpoint_summary.sequence_number,
            );
        };

        let system_state = get_sui_system_state(&transaction.output_objects.as_slice())
            .context("Failed to find system state object output from end of epoch transaction")?;

        Ok(vec![StoredEpochStart {
            epoch: system_state.epoch() as i64,
            protocol_version: system_state.protocol_version() as i64,
            cp_lo: checkpoint_summary.sequence_number as i64 + 1,
            start_timestamp_ms: system_state.epoch_start_timestamp_ms() as i64,
            reference_gas_price: system_state.reference_gas_price() as i64,
            system_state: bcs::to_bytes(&system_state)
                .context("Failed to serialize SystemState")?,
        }])
    }
}

#[async_trait::async_trait]
impl Handler for KvEpochStarts {
    const MIN_EAGER_ROWS: usize = 1;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(kv_epoch_starts::table)
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
            start: from_epoch,
            end: to_epoch,
        } = epoch_interval(conn, from..to_exclusive).await?;
        if from_epoch < to_epoch {
            let filter = kv_epoch_starts::table
                .filter(kv_epoch_starts::epoch.between(from_epoch as i64, to_epoch as i64 - 1));

            Ok(diesel::delete(filter).execute(conn).await?)
        } else {
            Ok(0)
        }
    }
}
