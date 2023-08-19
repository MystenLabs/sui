// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use sui_json_rpc_types::Checkpoint as RpcCheckpoint;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::EndOfEpochData;

use crate::errors::IndexerError;
use crate::schema::checkpoints::{self};

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = checkpoints)]
pub struct StoredCheckpoint {
    pub sequence_number: i64,
    pub checkpoint_digest: Vec<u8>,
    pub epoch: i64,
    pub tx_digests: Vec<Vec<u8>>,
    pub network_total_transactions: i64,
    pub previous_checkpoint_digest: Option<Vec<u8>>,
    pub end_of_epoch: bool,
    pub timestamp_ms: i64,
    pub total_gas_cost: i64,
    pub computation_cost: i64,
    pub storage_cost: i64,
    pub storage_rebate: i64,
    pub non_refundable_storage_fee: i64,
    pub checkpoint_commitments: Vec<u8>,
    pub validator_signature: Vec<u8>,
}

#[derive(Debug)]
pub struct IndexedCheckpoint {
    pub sequence_number: u64,
    pub checkpoint_digest: CheckpointDigest,
    pub epoch: u64,
    pub tx_digests: Vec<TransactionDigest>,
    pub network_total_transactions: u64,
    pub previous_checkpoint_digest: Option<CheckpointDigest>,
    pub end_of_epoch: bool,
    pub timestamp_ms: u64,
    pub total_gas_cost: i64, // total gas cost could be negative
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
    pub non_refundable_storage_fee: u64,
    pub checkpoint_commitments: Vec<u8>,
    pub validator_signature: Vec<u8>,
    pub successful_tx_num: usize,
}

impl From<&IndexedCheckpoint> for StoredCheckpoint {
    fn from(c: &IndexedCheckpoint) -> Self {
        Self {
            sequence_number: c.sequence_number as i64,
            checkpoint_digest: c.checkpoint_digest.clone().into_inner().to_vec(),
            epoch: c.epoch as i64,
            tx_digests: c
                .tx_digests
                .iter()
                .map(|tx| tx.clone().into_inner().to_vec())
                .collect(),
            network_total_transactions: c.network_total_transactions as i64,
            previous_checkpoint_digest: c
                .previous_checkpoint_digest
                .as_ref()
                .map(|d| d.clone().into_inner().to_vec()),
            end_of_epoch: c.end_of_epoch,
            timestamp_ms: c.timestamp_ms as i64,
            total_gas_cost: c.total_gas_cost,
            computation_cost: c.computation_cost as i64,
            storage_cost: c.storage_cost as i64,
            storage_rebate: c.storage_rebate as i64,
            non_refundable_storage_fee: c.non_refundable_storage_fee as i64,
            checkpoint_commitments: c.checkpoint_commitments.clone(),
            validator_signature: c.validator_signature.clone(),
        }
    }
}

impl StoredCheckpoint {
    pub fn into_rpc(
        self,
        end_of_epoch_data: Option<EndOfEpochData>,
    ) -> Result<RpcCheckpoint, IndexerError> {
        let parsed_digest: CheckpointDigest =
            bcs::from_bytes(&self.checkpoint_digest).map_err(|e| {
                IndexerError::SerdeError(format!(
                    "Failed to decode checkpoint digest: {:?} with err: {:?}",
                    self.checkpoint_digest, e
                ))
            })?;

        let parsed_previous_digest: Option<CheckpointDigest> = self
            .previous_checkpoint_digest
            .map(|digest| {
                bcs::from_bytes(&digest).map_err(|e| {
                    IndexerError::SerdeError(format!(
                        "Failed to decode previous checkpoint digest: {:?} with err: {:?}",
                        digest, e
                    ))
                })
            })
            .transpose()?;
        let transactions: Vec<TransactionDigest> = self
            .tx_digests
            .into_iter()
            .map(|tx| {
                bcs::from_bytes(&tx).map_err(|e| {
                    IndexerError::SerdeError(format!(
                        "Failed to decode transaction digest: {:?} with err: {:?}",
                        tx, e
                    ))
                })
            })
            .collect::<Result<Vec<TransactionDigest>, IndexerError>>()?;
        let validator_signature = bcs::from_bytes(&self.validator_signature).map_err(|e| {
            IndexerError::SerdeError(format!(
                "Failed to decode validator signature: {:?} with err: {:?}",
                self.validator_signature, e
            ))
        })?;

        let checkpoint_commitments =
            bcs::from_bytes(&self.checkpoint_commitments).map_err(|e| {
                IndexerError::SerdeError(format!(
                    "Failed to decode checkpoint commitments: {:?} with err: {:?}",
                    self.checkpoint_commitments, e
                ))
            })?;

        Ok(RpcCheckpoint {
            epoch: self.epoch as u64,
            sequence_number: self.sequence_number as u64,
            digest: parsed_digest,
            previous_digest: parsed_previous_digest,
            end_of_epoch_data,
            epoch_rolling_gas_cost_summary: GasCostSummary {
                computation_cost: self.computation_cost as u64,
                storage_cost: self.storage_cost as u64,
                storage_rebate: self.storage_rebate as u64,
                non_refundable_storage_fee: self.non_refundable_storage_fee as u64,
            },
            network_total_transactions: self.network_total_transactions as u64,
            timestamp_ms: self.timestamp_ms as u64,
            transactions,
            validator_signature,
            checkpoint_commitments,
        })
    }
}

// impl Checkpoint {
// // FIXME is this used at all?
// pub fn from(
//     rpc_checkpoint: &RpcCheckpoint,
//     successful_tx_num: i64,
// ) -> Result<Self, IndexerError> {
//     let total_gas_cost = rpc_checkpoint
//         .epoch_rolling_gas_cost_summary
//         .computation_cost as i64
//         + rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_cost as i64
//         - rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_rebate as i64;

//     let tx_digests = rpc_checkpoint
//         .transactions
//         .iter()
//         .map(|t| t.into_inner())
//         .collect::<Vec<_>>();

//     Ok(Checkpoint {
//         sequence_number: rpc_checkpoint.sequence_number as i64,
//         checkpoint_digest: rpc_checkpoint.digest.into_inner(),
//         epoch: rpc_checkpoint.epoch as i64,
//         tx_digests,
//         previous_checkpoint_digest: rpc_checkpoint.previous_digest.map(|d| d.into_inner()),
//         end_of_epoch: rpc_checkpoint.end_of_epoch_data.is_some(),
//         total_gas_cost,
//         computation_cost: rpc_checkpoint
//             .epoch_rolling_gas_cost_summary
//             .computation_cost as i64,
//         storage_cost: rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_cost as i64,
//         storage_rebate: rpc_checkpoint.epoch_rolling_gas_cost_summary.storage_rebate
//             as i64,
//         non_refundable_storage_fee: rpc_checkpoint.epoch_rolling_gas_cost_summary.non_refundable_storage_fee as i64,
//         successful_tx_num,
//         network_total_transactions: rpc_checkpoint.network_total_transactions as i64,
//         timestamp_ms: rpc_checkpoint.timestamp_ms as i64,
//         validator_signature: bcs::to_bytes(&rpc_checkpoint.validator_signature),
//         checkpoint_commitments: bcs::to_bytes(&rpc_checkpoint.checkpoint_commitments),
//     })
// }

impl IndexedCheckpoint {
    pub fn from_sui_checkpoint(
        checkpoint: &sui_types::messages_checkpoint::CertifiedCheckpointSummary,
        contents: &sui_types::messages_checkpoint::CheckpointContents,
        successful_tx_num: usize,
    ) -> Self {
        let total_gas_cost = checkpoint.epoch_rolling_gas_cost_summary.computation_cost as i64
            + checkpoint.epoch_rolling_gas_cost_summary.storage_cost as i64
            - checkpoint.epoch_rolling_gas_cost_summary.storage_rebate as i64;
        let tx_digests = contents
            .iter()
            .map(|t| t.transaction.clone())
            .collect::<Vec<_>>();
        let auth_sig = &checkpoint.auth_sig().signature;
        Self {
            sequence_number: checkpoint.sequence_number,
            checkpoint_digest: *checkpoint.digest(),
            epoch: checkpoint.epoch,
            tx_digests,
            previous_checkpoint_digest: checkpoint.previous_digest,
            end_of_epoch: checkpoint.end_of_epoch_data.is_some(),
            total_gas_cost,
            computation_cost: checkpoint.epoch_rolling_gas_cost_summary.computation_cost,
            storage_cost: checkpoint.epoch_rolling_gas_cost_summary.storage_cost,
            storage_rebate: checkpoint.epoch_rolling_gas_cost_summary.storage_rebate,
            non_refundable_storage_fee: checkpoint
                .epoch_rolling_gas_cost_summary
                .non_refundable_storage_fee,
            successful_tx_num,
            network_total_transactions: checkpoint.network_total_transactions,
            timestamp_ms: checkpoint.timestamp_ms,
            validator_signature: bcs::to_bytes(auth_sig).unwrap(),
            checkpoint_commitments: bcs::to_bytes(&checkpoint.checkpoint_commitments).unwrap(),
        }
    }
}
