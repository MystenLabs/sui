// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use sui_json_rpc_types::Checkpoint as RpcCheckpoint;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::EndOfEpochData;

use crate::schema::checkpoints;
use crate::schema::checkpoints::end_of_epoch_data;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = checkpoints)]
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
            checkpoint_commitments: vec![],
        })
    }
}

impl Checkpoint {
    pub fn from(
        rpc_checkpoint: &RpcCheckpoint,
        previous_checkpoint_commit: &Checkpoint,
    ) -> Result<Self, IndexerError> {
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

        Ok(Checkpoint {
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
            total_storage_rebate: rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_rebate
                as i64,

            total_transactions: checkpoint_transaction_count,
            total_transactions_from_genesis: rpc_checkpoint.network_total_transactions as i64,
            total_transactions_current_epoch: current_epoch_transaction_count,
            timestamp_ms: rpc_checkpoint.timestamp_ms as i64,
            timestamp_ms_str: NaiveDateTime::from_timestamp_millis(
                rpc_checkpoint.timestamp_ms as i64,
            )
            .unwrap(),
            checkpoint_tps: tps,
        })
    }
}
