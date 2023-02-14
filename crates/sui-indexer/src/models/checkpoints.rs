// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::checkpoints;
use crate::schema::checkpoints::dsl::{checkpoints as checkpoints_table, sequence_number};
// use crate::utils::log_errors_to_pg;
use crate::PgPoolConnection;

use diesel::prelude::*;

use sui_types::messages_checkpoint::CheckpointSummary;

#[derive(Queryable, Debug, Clone)]
pub struct Checkpoint {
    pub sequence_number: i64,
    pub content_digest: String,
    pub epoch: i64,
    pub total_gas_cost: i64,
    pub total_computation_cost: i64,
    pub total_storage_cost: i64,
    pub total_storage_rebate: i64,
    pub total_transactions: i64,
    pub previous_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = checkpoints)]
pub struct NewCheckpoint {
    pub sequence_number: i64,
    pub content_digest: String,
    pub epoch: i64,
    pub total_gas_cost: i64,
    pub total_computation_cost: i64,
    pub total_storage_cost: i64,
    pub total_storage_rebate: i64,
    pub total_transactions: i64,
    pub previous_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub timestamp_ms: i64,
}

pub fn commit_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
    checkpoint_summary: CheckpointSummary,
) -> Result<usize, IndexerError> {
    let total_gas_cost = checkpoint_summary
        .epoch_rolling_gas_cost_summary
        .computation_cost
        + checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .storage_cost
        - checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .storage_rebate;
    let next_committee_json = checkpoint_summary.end_of_epoch_data.map(|e| {
        serde_json::to_string(&e.next_epoch_committee)
            .expect("Failed to serialize next_epoch_committee to JSON")
    });

    let checkpoint = NewCheckpoint {
        sequence_number: checkpoint_summary.sequence_number as i64,
        content_digest: checkpoint_summary.content_digest.base58_encode(),
        epoch: checkpoint_summary.epoch as i64,
        total_gas_cost: total_gas_cost as i64,
        total_computation_cost: checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .computation_cost as i64,
        total_storage_cost: checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .storage_cost as i64,
        total_storage_rebate: checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .storage_rebate as i64,
        total_transactions: checkpoint_summary.network_total_transactions as i64,
        previous_digest: checkpoint_summary
            .previous_digest
            .map(|d| d.base58_encode()),
        next_epoch_committee: next_committee_json,
        timestamp_ms: checkpoint_summary.timestamp_ms as i64,
    };
    commit_checkpoint_impl(pg_pool_conn, checkpoint)
}

fn commit_checkpoint_impl(
    pg_pool_conn: &mut PgPoolConnection,
    checkpoint: NewCheckpoint,
) -> Result<usize, IndexerError> {
    let checkpoint_commit_result = diesel::insert_into(checkpoints_table)
        .values(checkpoint.clone())
        .on_conflict(sequence_number)
        .do_nothing()
        .execute(pg_pool_conn);
    checkpoint_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing checkpoint to PostgresDB with events {:?} and error: {:?}",
            checkpoint, e
        ))
    })
}
