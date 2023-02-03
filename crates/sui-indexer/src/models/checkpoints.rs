// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::checkpoints;
use crate::schema::checkpoints::dsl::{checkpoints as checkpoints_table, sequence_number};
// use crate::utils::log_errors_to_pg;
use crate::errors::IndexerError;
use crate::PgPoolConnection;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::Error;

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
    pub total_transactions_current_epoch: i64,
    pub total_transactions_from_genesis: i64,
    pub previous_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub timestamp_ms: i64,
    pub timestamp_ms_str: NaiveDateTime,
    pub checkpoint_tps: f32,
}

impl Default for Checkpoint {
    fn default() -> Checkpoint {
        Checkpoint {
            sequence_number: 0,
            content_digest: String::from(""),
            epoch: 0,
            total_gas_cost: 0,
            total_computation_cost: 0,
            total_storage_cost: 0,
            total_storage_rebate: 0,
            total_transactions: 0,
            total_transactions_current_epoch: 0,
            total_transactions_from_genesis: 0,
            previous_digest: None,
            next_epoch_committee: None,
            timestamp_ms: 0,
            timestamp_ms_str: NaiveDateTime::from_timestamp_millis(0).unwrap(),
            checkpoint_tps: 0.0,
        }
    }
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
    pub total_transactions_current_epoch: i64,
    pub total_transactions_from_genesis: i64,
    pub previous_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub timestamp_ms: i64,
    pub timestamp_ms_str: NaiveDateTime,
    pub checkpoint_tps: f32,
}

impl From<NewCheckpoint> for Checkpoint {
    fn from(new_checkpoint: NewCheckpoint) -> Self {
        Checkpoint {
            sequence_number: new_checkpoint.sequence_number,
            content_digest: new_checkpoint.content_digest,
            epoch: new_checkpoint.epoch,
            total_gas_cost: new_checkpoint.total_gas_cost,
            total_computation_cost: new_checkpoint.total_computation_cost,
            total_storage_cost: new_checkpoint.total_storage_cost,
            total_storage_rebate: new_checkpoint.total_storage_rebate,
            total_transactions: new_checkpoint.total_transactions,
            total_transactions_current_epoch: new_checkpoint.total_transactions_current_epoch,
            total_transactions_from_genesis: new_checkpoint.total_transactions_from_genesis,
            previous_digest: new_checkpoint.previous_digest,
            next_epoch_committee: new_checkpoint.next_epoch_committee,
            timestamp_ms: new_checkpoint.timestamp_ms,
            timestamp_ms_str: new_checkpoint.timestamp_ms_str,
            checkpoint_tps: new_checkpoint.checkpoint_tps,
        }
    }
}

pub fn create_checkpoint(
    checkpoint_summary: CheckpointSummary,
    previous_checkpoint_commit: Checkpoint,
) -> NewCheckpoint {
    let total_gas_cost = checkpoint_summary
        .epoch_rolling_gas_cost_summary
        .computation_cost
        + checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .storage_cost
        - checkpoint_summary
            .epoch_rolling_gas_cost_summary
            .storage_rebate;
    let next_committee_json = checkpoint_summary.next_epoch_committee.map(|c| {
        serde_json::to_string(&c).expect("Failed to serialize next_epoch_committee to JSON")
    });

    // Unsure how to calculate TPS for first item
    let mut tps = 0.0;
    let mut checkpoint_transaction_count = checkpoint_summary.network_total_transactions as i64;
    let mut current_epoch_transaction_count = checkpoint_summary.network_total_transactions as i64;

    if checkpoint_summary.sequence_number != 0 {
        tps = (checkpoint_summary.network_total_transactions as f32
            - previous_checkpoint_commit.total_transactions_from_genesis as f32)
            / ((checkpoint_summary.timestamp_ms - previous_checkpoint_commit.timestamp_ms as u64)
                as f32
                / 1000.0) as f32;

        checkpoint_transaction_count = checkpoint_summary.network_total_transactions as i64
            - previous_checkpoint_commit.total_transactions_from_genesis;

        current_epoch_transaction_count =
            if previous_checkpoint_commit.next_epoch_committee.is_none() {
                previous_checkpoint_commit.total_transactions_current_epoch
                    + checkpoint_transaction_count
            } else {
                checkpoint_transaction_count
            };
    }

    NewCheckpoint {
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
        total_transactions: checkpoint_transaction_count,
        total_transactions_from_genesis: checkpoint_summary.network_total_transactions as i64,
        total_transactions_current_epoch: current_epoch_transaction_count,
        previous_digest: checkpoint_summary
            .previous_digest
            .map(|d| d.base58_encode()),
        next_epoch_committee: next_committee_json,
        timestamp_ms: checkpoint_summary.timestamp_ms as i64,
        timestamp_ms_str: NaiveDateTime::from_timestamp_millis(
            checkpoint_summary.timestamp_ms as i64,
        )
        .unwrap(),
        checkpoint_tps: tps,
    }
}

pub fn commit_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
    checkpoint: NewCheckpoint,
) -> Result<usize, IndexerError> {
    commit_checkpoint_impl(pg_pool_conn, checkpoint)
}

pub fn read_previous_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
    currency_checkpoint_sequence_number: i64,
) -> Result<Checkpoint, IndexerError> {
    let checkpoint_read_result: Result<Checkpoint, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            checkpoints_table
                .filter(sequence_number.eq(currency_checkpoint_sequence_number - 1))
                .limit(1)
                .first::<Checkpoint>(conn)
        });
    checkpoint_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading previous checkpoint in PostgresDB with error {:?}",
            e
        ))
    })
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
