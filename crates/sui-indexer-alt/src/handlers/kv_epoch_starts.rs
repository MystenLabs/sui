// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::handler::Handler;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_indexer_alt_framework::types::sui_system_state::SuiSystemStateTrait;
use sui_indexer_alt_framework::types::sui_system_state::get_sui_system_state;
use sui_indexer_alt_framework::types::transaction::TransactionKind;
use sui_indexer_alt_schema::epochs::StoredEpochStart;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::cp_sequence_numbers::epoch_interval;

pub(crate) struct KvEpochStarts;

#[async_trait]
impl Processor for KvEpochStarts {
    const NAME: &'static str = "kv_epoch_starts";

    type Value = StoredEpochStart;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            summary,
            transactions,
            ..
        } = checkpoint.as_ref();

        // If this is the last checkpoint in the current epoch, it will contain enough information
        // about the start of the next epoch.
        if !summary.is_last_checkpoint_of_epoch() {
            return Ok(vec![]);
        }

        let Some(transaction) = transactions.iter().find(|tx| {
            matches!(
                tx.transaction.kind(),
                TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
            )
        }) else {
            bail!(
                "Failed to get end of epoch transaction in checkpoint {} with EndOfEpochData",
                summary.sequence_number,
            );
        };

        let output_objects: Vec<_> = transaction
            .output_objects(&checkpoint.object_set)
            .cloned()
            .collect();

        let system_state = get_sui_system_state(&output_objects.as_slice())
            .context("Failed to find system state object output from end of epoch transaction")?;

        Ok(vec![StoredEpochStart {
            epoch: system_state.epoch() as i64,
            protocol_version: system_state.protocol_version() as i64,
            cp_lo: summary.sequence_number as i64 + 1,
            start_timestamp_ms: system_state.epoch_start_timestamp_ms() as i64,
            reference_gas_price: system_state.reference_gas_price() as i64,
            system_state: bcs::to_bytes(&system_state)
                .context("Failed to serialize SystemState")?,
        }])
    }
}

#[async_trait]
impl Handler for KvEpochStarts {
    const MIN_EAGER_ROWS: usize = 1;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(kv_epoch_starts::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
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
