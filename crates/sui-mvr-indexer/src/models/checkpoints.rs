// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use sui_json_rpc_types::Checkpoint as RpcCheckpoint;
use sui_types::base_types::TransactionDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::gas::GasCostSummary;

use crate::errors::IndexerError;
use crate::schema::{chain_identifier, checkpoints, pruner_cp_watermark};
use crate::types::IndexedCheckpoint;

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = chain_identifier)]
pub struct StoredChainIdentifier {
    pub checkpoint_digest: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = checkpoints)]
pub struct StoredCheckpoint {
    pub sequence_number: i64,
    pub checkpoint_digest: Vec<u8>,
    pub epoch: i64,
    pub network_total_transactions: i64,
    pub previous_checkpoint_digest: Option<Vec<u8>>,
    pub end_of_epoch: bool,
    pub tx_digests: Vec<Option<Vec<u8>>>,
    pub timestamp_ms: i64,
    pub total_gas_cost: i64,
    pub computation_cost: i64,
    pub storage_cost: i64,
    pub storage_rebate: i64,
    pub non_refundable_storage_fee: i64,
    pub checkpoint_commitments: Vec<u8>,
    pub validator_signature: Vec<u8>,
    pub end_of_epoch_data: Option<Vec<u8>>,
    pub min_tx_sequence_number: Option<i64>,
    pub max_tx_sequence_number: Option<i64>,
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
                .map(|tx| Some(tx.into_inner().to_vec()))
                .collect(),
            network_total_transactions: c.network_total_transactions as i64,
            previous_checkpoint_digest: c
                .previous_checkpoint_digest
                .as_ref()
                .map(|d| (*d).into_inner().to_vec()),
            timestamp_ms: c.timestamp_ms as i64,
            total_gas_cost: c.total_gas_cost,
            computation_cost: c.computation_cost as i64,
            storage_cost: c.storage_cost as i64,
            storage_rebate: c.storage_rebate as i64,
            non_refundable_storage_fee: c.non_refundable_storage_fee as i64,
            checkpoint_commitments: bcs::to_bytes(&c.checkpoint_commitments).unwrap(),
            validator_signature: bcs::to_bytes(&c.validator_signature).unwrap(),
            end_of_epoch_data: c
                .end_of_epoch_data
                .as_ref()
                .map(|d| bcs::to_bytes(d).unwrap()),
            end_of_epoch: c.end_of_epoch_data.is_some(),
            min_tx_sequence_number: Some(c.min_tx_sequence_number as i64),
            max_tx_sequence_number: Some(c.max_tx_sequence_number as i64),
        }
    }
}

impl TryFrom<StoredCheckpoint> for RpcCheckpoint {
    type Error = IndexerError;
    fn try_from(checkpoint: StoredCheckpoint) -> Result<RpcCheckpoint, IndexerError> {
        let parsed_digest = CheckpointDigest::try_from(checkpoint.checkpoint_digest.clone())
            .map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to decode checkpoint digest: {:?} with err: {:?}",
                    checkpoint.checkpoint_digest, e
                ))
            })?;

        let parsed_previous_digest: Option<CheckpointDigest> = checkpoint
            .previous_checkpoint_digest
            .map(|digest| {
                CheckpointDigest::try_from(digest.clone()).map_err(|e| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "Failed to decode previous checkpoint digest: {:?} with err: {:?}",
                        digest, e
                    ))
                })
            })
            .transpose()?;

        let transactions: Vec<TransactionDigest> = {
            checkpoint
                .tx_digests
                .into_iter()
                .map(|tx_digest| match tx_digest {
                    None => Err(IndexerError::PersistentStorageDataCorruptionError(
                        "tx_digests should not contain null elements".to_string(),
                    )),
                    Some(tx_digest) => {
                        TransactionDigest::try_from(tx_digest.as_slice()).map_err(|e| {
                            IndexerError::PersistentStorageDataCorruptionError(format!(
                                "Failed to decode transaction digest: {:?} with err: {:?}",
                                tx_digest, e
                            ))
                        })
                    }
                })
                .collect::<Result<Vec<TransactionDigest>, IndexerError>>()?
        };
        let validator_signature =
            bcs::from_bytes(&checkpoint.validator_signature).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to decode validator signature: {:?} with err: {:?}",
                    checkpoint.validator_signature, e
                ))
            })?;

        let checkpoint_commitments =
            bcs::from_bytes(&checkpoint.checkpoint_commitments).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to decode checkpoint commitments: {:?} with err: {:?}",
                    checkpoint.checkpoint_commitments, e
                ))
            })?;

        let end_of_epoch_data = checkpoint
            .end_of_epoch_data
            .map(|data| {
                bcs::from_bytes(&data).map_err(|e| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "Failed to decode end of epoch data: {:?} with err: {:?}",
                        data, e
                    ))
                })
            })
            .transpose()?;

        Ok(RpcCheckpoint {
            epoch: checkpoint.epoch as u64,
            sequence_number: checkpoint.sequence_number as u64,
            digest: parsed_digest,
            previous_digest: parsed_previous_digest,
            end_of_epoch_data,
            epoch_rolling_gas_cost_summary: GasCostSummary {
                computation_cost: checkpoint.computation_cost as u64,
                storage_cost: checkpoint.storage_cost as u64,
                storage_rebate: checkpoint.storage_rebate as u64,
                non_refundable_storage_fee: checkpoint.non_refundable_storage_fee as u64,
            },
            network_total_transactions: checkpoint.network_total_transactions as u64,
            timestamp_ms: checkpoint.timestamp_ms as u64,
            transactions,
            validator_signature,
            checkpoint_commitments,
        })
    }
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = pruner_cp_watermark)]
pub struct StoredCpTx {
    pub checkpoint_sequence_number: i64,
    pub min_tx_sequence_number: i64,
    pub max_tx_sequence_number: i64,
}

impl From<&IndexedCheckpoint> for StoredCpTx {
    fn from(c: &IndexedCheckpoint) -> Self {
        Self {
            checkpoint_sequence_number: c.sequence_number as i64,
            min_tx_sequence_number: c.min_tx_sequence_number as i64,
            max_tx_sequence_number: c.max_tx_sequence_number as i64,
        }
    }
}
