// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::{
    BalanceChange, ObjectChange, SuiTransaction, SuiTransactionEffects, SuiTransactionEvents,
    SuiTransactionResponse,
};
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[derive(Debug, Clone)]
pub struct SuiTransactionFullResponse {
    pub digest: TransactionDigest,
    /// Transaction input data
    pub transaction: SuiTransaction,
    pub effects: SuiTransactionEffects,
    pub events: SuiTransactionEvents,
    pub object_changes: Option<Vec<ObjectChange>>,
    pub balance_changes: Option<Vec<BalanceChange>>,
    pub timestamp_ms: u64,
    pub confirmed_local_execution: Option<bool>,
    pub checkpoint: CheckpointSequenceNumber,
}

impl TryFrom<SuiTransactionResponse> for SuiTransactionFullResponse {
    type Error = anyhow::Error;

    fn try_from(response: SuiTransactionResponse) -> Result<Self, Self::Error> {
        let SuiTransactionResponse {
            digest,
            transaction,
            effects,
            events,
            object_changes,
            balance_changes,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
            errors,
        } = response;

        let transaction = transaction.ok_or_else(|| {
            anyhow::anyhow!(
                "Transaction is None in SuiTransactionFullResponse of digest {:?}.",
                digest
            )
        })?;
        let effects = effects.ok_or_else(|| {
            anyhow::anyhow!(
                "Effects is None in SuiTransactionFullResponse of digest {:?}.",
                digest
            )
        })?;
        let events = events.ok_or_else(|| {
            anyhow::anyhow!(
                "Events is None in SuiTransactionFullResponse of digest {:?}.",
                digest
            )
        })?;
        let timestamp_ms = timestamp_ms.ok_or_else(|| {
            anyhow::anyhow!(
                "TimestampMs is None in SuiTransactionFullResponse of digest {:?}.",
                digest
            )
        })?;
        let checkpoint = checkpoint.ok_or_else(|| {
            anyhow::anyhow!(
                "Checkpoint is None in SuiTransactionFullResponse of digest {:?}.",
                digest
            )
        })?;
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Errors in SuiTransactionFullResponse of digest {:?}: {:?}",
                digest,
                errors
            ));
        }

        Ok(SuiTransactionFullResponse {
            digest,
            transaction,
            effects,
            events,
            object_changes,
            balance_changes,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
        })
    }
}

impl From<SuiTransactionFullResponse> for SuiTransactionResponse {
    fn from(response: SuiTransactionFullResponse) -> Self {
        let SuiTransactionFullResponse {
            digest,
            transaction,
            effects,
            events,
            object_changes,
            balance_changes,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
        } = response;

        SuiTransactionResponse {
            digest,
            transaction: Some(transaction),
            effects: Some(effects),
            events: Some(events),
            object_changes,
            balance_changes,
            timestamp_ms: Some(timestamp_ms),
            confirmed_local_execution,
            checkpoint: Some(checkpoint),
            errors: vec![],
        }
    }
}
