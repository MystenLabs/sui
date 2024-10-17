// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use tap::Pipe;

use sui_json_rpc_types::Checkpoint as RpcCheckpoint;
use sui_types::messages_checkpoint::CheckpointContents;

use crate::errors::IndexerError;
use crate::schema::raw_checkpoints;
use crate::types::IndexedCheckpoint;

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = raw_checkpoints)]
pub struct StoredRawCheckpoint {
    pub sequence_number: i64,
    /// BCS serialized CertifiedCheckpointSummary
    pub certified_checkpoint: Vec<u8>,
    /// BCS serialized CheckpointContents
    pub checkpoint_contents: Vec<u8>,
}

impl From<&IndexedCheckpoint> for StoredRawCheckpoint {
    fn from(c: &IndexedCheckpoint) -> Self {
        Self {
            sequence_number: c.sequence_number as i64,
            certified_checkpoint: bcs::to_bytes(c.certified_checkpoint.as_ref().unwrap()).unwrap(),
            checkpoint_contents: bcs::to_bytes(c.checkpoint_contents.as_ref().unwrap()).unwrap(),
        }
    }
}

impl TryFrom<StoredRawCheckpoint> for RpcCheckpoint {
    type Error = IndexerError;

    fn try_from(raw_cp: StoredRawCheckpoint) -> Result<RpcCheckpoint, IndexerError> {
        let checkpoint_contents = raw_cp
            .checkpoint_contents
            .pipe(|contents| bcs::from_bytes::<CheckpointContents>(&contents))
            .map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to decode checkpoint contents: {e}"
                ))
            })?;

        let certified_checkpoint = raw_cp
            .certified_checkpoint
            .pipe(|bytes| {
                bcs::from_bytes::<sui_types::messages_checkpoint::CertifiedCheckpointSummary>(
                    &bytes,
                )
            })
            .map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to decode certified checkpoint summary: {e}"
                ))
            })?;

        Ok(RpcCheckpoint {
            epoch: certified_checkpoint.epoch,
            sequence_number: certified_checkpoint.sequence_number,
            digest: *certified_checkpoint.digest(),
            network_total_transactions: certified_checkpoint.network_total_transactions,
            previous_digest: certified_checkpoint.previous_digest,
            epoch_rolling_gas_cost_summary: certified_checkpoint
                .epoch_rolling_gas_cost_summary
                .clone(),
            timestamp_ms: certified_checkpoint.timestamp_ms,
            checkpoint_commitments: certified_checkpoint.checkpoint_commitments.clone(),
            validator_signature: certified_checkpoint.auth_sig().signature.clone(),
            end_of_epoch_data: certified_checkpoint.end_of_epoch_data.clone(),
            transactions: checkpoint_contents.iter().map(|t| t.transaction).collect(),
        })
    }
}
