// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use sui_json_rpc_types::Checkpoint as RpcCheckpoint;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::EndOfEpochData;

use crate::errors::IndexerError;
use crate::schema_v2::checkpoints;
use crate::types_v2::IndexedCheckpoint;

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

impl From<&IndexedCheckpoint> for StoredCheckpoint {
    fn from(c: &IndexedCheckpoint) -> Self {
        Self {
            sequence_number: c.sequence_number as i64,
            checkpoint_digest: c.checkpoint_digest.into_inner().to_vec(),
            epoch: c.epoch as i64,
            tx_digests: c
                .tx_digests
                .iter()
                .map(|tx| tx.into_inner().to_vec())
                .collect(),
            network_total_transactions: c.network_total_transactions as i64,
            previous_checkpoint_digest: c
                .previous_checkpoint_digest
                .as_ref()
                .map(|d| (*d).into_inner().to_vec()),
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
