// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::NaiveDateTime;
use diesel::dsl::max;
use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::Checkpoint as RpcCheckpoint;

use crate::errors::IndexerError;
use crate::schema::checkpoints;
use crate::schema::checkpoints::dsl::{checkpoints as checkpoints_table, sequence_number};
use crate::PgPoolConnection;

#[derive(Queryable, Debug, Clone)]
pub struct Checkpoint {
    pub sequence_number: i64,
    pub checkpoint_digest: String,
    pub epoch: i64,
    pub transactions: Vec<Option<String>>,
    pub previous_checkpoint_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub next_epoch_protocol_version: Option<i64>,
    pub total_gas_cost: i64,
    pub total_computation_cost: i64,
    pub total_storage_cost: i64,
    pub total_storage_rebate: i64,
    pub total_transactions: i64,
    pub total_transactions_current_epoch: i64,
    pub total_transactions_from_genesis: i64,
    pub timestamp_ms: i64,
    pub timestamp_ms_str: NaiveDateTime,
    pub checkpoint_tps: f32,
}

impl Default for Checkpoint {
    fn default() -> Checkpoint {
        Checkpoint {
            sequence_number: 0,
            checkpoint_digest: "".into(),
            epoch: 0,
            transactions: vec![],
            previous_checkpoint_digest: None,
            next_epoch_committee: None,
            next_epoch_protocol_version: None,
            total_gas_cost: 0,
            total_computation_cost: 0,
            total_storage_cost: 0,
            total_storage_rebate: 0,
            total_transactions: 0,
            total_transactions_current_epoch: 0,
            total_transactions_from_genesis: 0,
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
    pub checkpoint_digest: String,
    pub epoch: i64,
    pub transactions: Vec<Option<String>>,
    pub previous_checkpoint_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub next_epoch_protocol_version: Option<i64>,
    pub total_gas_cost: i64,
    pub total_computation_cost: i64,
    pub total_storage_cost: i64,
    pub total_storage_rebate: i64,
    pub total_transactions: i64,
    pub total_transactions_current_epoch: i64,
    pub total_transactions_from_genesis: i64,
    pub timestamp_ms: i64,
    pub timestamp_ms_str: NaiveDateTime,
    pub checkpoint_tps: f32,
}

impl From<NewCheckpoint> for Checkpoint {
    fn from(new_checkpoint: NewCheckpoint) -> Self {
        Checkpoint {
            sequence_number: new_checkpoint.sequence_number,
            checkpoint_digest: new_checkpoint.checkpoint_digest,
            epoch: new_checkpoint.epoch,
            transactions: new_checkpoint.transactions,
            previous_checkpoint_digest: new_checkpoint.previous_checkpoint_digest,
            next_epoch_committee: new_checkpoint.next_epoch_committee,
            next_epoch_protocol_version: new_checkpoint.next_epoch_protocol_version,
            total_gas_cost: new_checkpoint.total_gas_cost,
            total_computation_cost: new_checkpoint.total_computation_cost,
            total_storage_cost: new_checkpoint.total_storage_cost,
            total_storage_rebate: new_checkpoint.total_storage_rebate,
            total_transactions: new_checkpoint.total_transactions,
            total_transactions_current_epoch: new_checkpoint.total_transactions_current_epoch,
            total_transactions_from_genesis: new_checkpoint.total_transactions_from_genesis,
            timestamp_ms: new_checkpoint.timestamp_ms,
            timestamp_ms_str: new_checkpoint.timestamp_ms_str,
            checkpoint_tps: new_checkpoint.checkpoint_tps,
        }
    }
}

pub fn create_checkpoint(
    rpc_checkpoint: RpcCheckpoint,
    previous_checkpoint_commit: Checkpoint,
) -> NewCheckpoint {
    let total_gas_cost = rpc_checkpoint
        .epoch_rolling_gas_cost_summary
        .computation_cost
        + rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_cost
        - rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_rebate;

    let next_committee_json = rpc_checkpoint.end_of_epoch_data.clone().map(|e| {
        serde_json::to_string(&e.next_epoch_committee)
            .expect("Failed to serialize next_epoch_committee to JSON")
    });
    let next_epoch_version = rpc_checkpoint
        .end_of_epoch_data
        .map(|e| e.next_epoch_protocol_version.as_u64() as i64);

    // TPS of the first row is always 0
    let mut tps = 0.0;
    let mut checkpoint_transaction_count = rpc_checkpoint.network_total_transactions as i64;
    let mut current_epoch_transaction_count = rpc_checkpoint.network_total_transactions as i64;

    if rpc_checkpoint.sequence_number != 0 {
        tps = (rpc_checkpoint.network_total_transactions as f32
            - previous_checkpoint_commit.total_transactions_from_genesis as f32)
            / ((rpc_checkpoint.timestamp_ms - previous_checkpoint_commit.timestamp_ms as u64)
                as f32
                / 1000.0);

        checkpoint_transaction_count = rpc_checkpoint.network_total_transactions as i64
            - previous_checkpoint_commit.total_transactions_from_genesis;

        current_epoch_transaction_count =
            if previous_checkpoint_commit.next_epoch_committee.is_none() {
                previous_checkpoint_commit.total_transactions_current_epoch
                    + checkpoint_transaction_count
            } else {
                checkpoint_transaction_count
            };
    }
    let checkpoint_transactions: Vec<Option<String>> = rpc_checkpoint
        .transactions
        .iter()
        .map(|t| Some(t.base58_encode()))
        .collect();

    NewCheckpoint {
        sequence_number: rpc_checkpoint.sequence_number as i64,
        checkpoint_digest: rpc_checkpoint.digest.base58_encode(),
        epoch: rpc_checkpoint.epoch as i64,
        transactions: checkpoint_transactions,
        previous_checkpoint_digest: rpc_checkpoint.previous_digest.map(|d| d.base58_encode()),

        next_epoch_committee: next_committee_json,
        next_epoch_protocol_version: next_epoch_version,

        total_gas_cost: total_gas_cost as i64,
        total_computation_cost: rpc_checkpoint
            .epoch_rolling_gas_cost_summary
            .computation_cost as i64,
        total_storage_cost: rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_cost as i64,
        total_storage_rebate: rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_rebate as i64,

        total_transactions: checkpoint_transaction_count,
        total_transactions_from_genesis: rpc_checkpoint.network_total_transactions as i64,
        total_transactions_current_epoch: current_epoch_transaction_count,
        timestamp_ms: rpc_checkpoint.timestamp_ms as i64,
        timestamp_ms_str: NaiveDateTime::from_timestamp_millis(rpc_checkpoint.timestamp_ms as i64)
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

pub fn get_previous_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
    currency_checkpoint_sequence_number: i64,
) -> Result<Checkpoint, IndexerError> {
    let checkpoint_read_result = pg_pool_conn
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

pub fn get_latest_checkpoint_sequence_number(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<i64, IndexerError> {
    let latest_checkpoint_sequence_number_read_result: Result<i64, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            checkpoints_table
                .select(max(sequence_number))
                .first::<Option<i64>>(conn)
                .map(|o| o.unwrap_or(0))
        });
    latest_checkpoint_sequence_number_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading latest checkpoint sequence number in PostgresDB with error {:?}",
            e
        ))
    })
}
