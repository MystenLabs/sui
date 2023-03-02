// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::NaiveDateTime;
use diesel::dsl::max;
use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::{Checkpoint as RpcCheckpoint, CheckpointId};
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::gas::GasCostSummary;

use crate::errors::IndexerError;
use crate::schema::checkpoints::dsl::{checkpoints as checkpoints_table, sequence_number};
use crate::schema::checkpoints::{self, checkpoint_digest, end_of_epoch_data};
use crate::PgPoolConnection;
use sui_types::messages_checkpoint::EndOfEpochData;

#[derive(Queryable, Debug, Clone)]
pub struct Checkpoint {
    pub sequence_number: i64,
    pub checkpoint_digest: String,
    pub epoch: i64,
    pub transactions: Vec<Option<String>>,
    pub previous_checkpoint_digest: Option<String>,
    pub next_epoch_committee: Option<String>,
    pub next_epoch_protocol_version: Option<i64>,
    pub end_of_epoch_data: Option<String>,
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
            end_of_epoch_data: None,
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
    pub end_of_epoch_data: Option<String>,
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
            end_of_epoch_data: new_checkpoint.end_of_epoch_data,
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

impl TryFrom<Checkpoint> for RpcCheckpoint {
    type Error = IndexerError;
    fn try_from(checkpoint: Checkpoint) -> Result<Self, Self::Error> {
        let parsed_digest = checkpoint
            .checkpoint_digest
            .parse::<CheckpointDigest>()
            .map_err(|e| {
                IndexerError::JsonSerdeError(format!(
                    "Failed to decode checkpoint digest: {:?} with err: {:?}",
                    checkpoint.checkpoint_digest, e
                ))
            })?;

        let parsed_previous_digest = checkpoint
            .previous_checkpoint_digest
            .map(|digest| {
                digest.parse::<CheckpointDigest>().map_err(|e| {
                    IndexerError::JsonSerdeError(format!(
                        "Failed to decode previous checkpoint digest: {:?} with err: {:?}",
                        digest, e
                    ))
                })
            })
            .transpose()?;
        let parsed_txn_digests: Vec<TransactionDigest> = checkpoint
            .transactions
            .into_iter()
            .filter_map(|txn| {
                txn.map(|txn| {
                    txn.parse().map_err(|e| {
                        IndexerError::JsonSerdeError(format!(
                            "Failed to decode transaction digest: {:?} with err: {:?}",
                            txn, e
                        ))
                    })
                })
            })
            .collect::<Result<Vec<TransactionDigest>, IndexerError>>()?;

        let data: Option<EndOfEpochData> =
            if let Some(end_of_epoch_data_str) = checkpoint.end_of_epoch_data {
                Some(serde_json::from_str(&end_of_epoch_data_str).map_err(|e| {
                    IndexerError::JsonSerdeError(format!(
                        "Failed to decode end_of_epoch_data: {:?} with err: {:?}",
                        end_of_epoch_data, e
                    ))
                })?)
            } else {
                None
            };

        Ok(RpcCheckpoint {
            epoch: checkpoint.epoch as u64,
            sequence_number: checkpoint.sequence_number as u64,
            digest: parsed_digest,
            previous_digest: parsed_previous_digest,
            end_of_epoch_data: data,
            epoch_rolling_gas_cost_summary: GasCostSummary {
                computation_cost: checkpoint.total_computation_cost as u64,
                storage_cost: checkpoint.total_storage_cost as u64,
                storage_rebate: checkpoint.total_storage_rebate as u64,
            },
            network_total_transactions: checkpoint.total_transactions_from_genesis as u64,
            timestamp_ms: checkpoint.timestamp_ms as u64,
            transactions: parsed_txn_digests,
        })
    }
}

pub fn create_checkpoint(
    rpc_checkpoint: RpcCheckpoint,
    previous_checkpoint_commit: Checkpoint,
) -> Result<NewCheckpoint, IndexerError> {
    let total_gas_cost = rpc_checkpoint
        .epoch_rolling_gas_cost_summary
        .computation_cost
        + rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_cost
        - rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_rebate;

    let end_of_epoch_data_json = rpc_checkpoint
        .end_of_epoch_data
        .clone()
        .map(|data| {
            serde_json::to_string(&data).map_err(|e| {
                IndexerError::JsonSerdeError(format!(
                    "Failed to serialize end_of_epoch_data to JSON: {:?}",
                    e
                ))
            })
        })
        .transpose()?;
    let next_epoch_committee_json = rpc_checkpoint
        .end_of_epoch_data
        .clone()
        .map(|data| {
            serde_json::to_string(&data.next_epoch_committee).map_err(|e| {
                IndexerError::JsonSerdeError(format!(
                    "Failed to serialize next_epoch_committee to JSON: {:?}",
                    e
                ))
            })
        })
        .transpose()?;
    let next_epoch_version = rpc_checkpoint
        .end_of_epoch_data
        .clone()
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

    Ok(NewCheckpoint {
        sequence_number: rpc_checkpoint.sequence_number as i64,
        checkpoint_digest: rpc_checkpoint.digest.base58_encode(),
        epoch: rpc_checkpoint.epoch as i64,
        transactions: checkpoint_transactions,
        previous_checkpoint_digest: rpc_checkpoint.previous_digest.map(|d| d.base58_encode()),

        next_epoch_committee: next_epoch_committee_json,
        next_epoch_protocol_version: next_epoch_version,
        end_of_epoch_data: end_of_epoch_data_json,

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
    })
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

pub fn get_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
    checkpoint_sequence_number: i64,
) -> Result<Checkpoint, IndexerError> {
    let checkpoint_read_result = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            checkpoints_table
                .filter(sequence_number.eq(checkpoint_sequence_number))
                .limit(1)
                .first::<Checkpoint>(conn)
        });
    checkpoint_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading checkpoint from PostgresDB with error {:?}",
            e
        ))
    })
}

pub fn get_checkpoint_from_digest(
    pg_pool_conn: &mut PgPoolConnection,
    checkpoint_digest_str: String,
) -> Result<Checkpoint, IndexerError> {
    let checkpoint_read_result = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            checkpoints_table
                .filter(checkpoint_digest.eq(checkpoint_digest_str.clone()))
                .limit(1)
                .first::<Checkpoint>(conn)
        });
    checkpoint_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading checkpoint from PostgresDB with digest {:?} and error {:?}",
            checkpoint_digest_str, e
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
                // -1 to differentiate between no checkpoints and the first checkpoint
                .map(|o| o.unwrap_or(-1))
        });
    latest_checkpoint_sequence_number_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading latest checkpoint sequence number in PostgresDB with error {:?}",
            e
        ))
    })
}

pub fn get_rpc_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
    checkpoint_id: CheckpointId,
) -> Result<RpcCheckpoint, IndexerError> {
    let checkpoint = match checkpoint_id {
        CheckpointId::SequenceNumber(seq) => get_checkpoint(pg_pool_conn, seq as i64)?,
        CheckpointId::Digest(digest) => {
            get_checkpoint_from_digest(pg_pool_conn, digest.base58_encode())?
        }
    };
    checkpoint.try_into()
}
