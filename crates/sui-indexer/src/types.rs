// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::{
    BalanceChange, ObjectChange, SuiCommand, SuiTransactionBlock, SuiTransactionBlockDataAPI,
    SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI, SuiTransactionBlockEvents,
    SuiTransactionBlockKind, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Owner;
use sui_types::transaction::{SenderSignedData, TransactionDataAPI};

use crate::errors::IndexerError;
use crate::models::addresses::Address;
use crate::models::transaction_index::{InputObject, MoveCall, Recipient};

pub struct FastPathTransactionBlockResponse {
    pub digest: TransactionDigest,
    pub transaction: SuiTransactionBlock,
    pub raw_transaction: Vec<u8>,
    pub effects: SuiTransactionBlockEffects,
    pub events: SuiTransactionBlockEvents,
    pub object_changes: Vec<ObjectChange>,
    pub balance_changes: Vec<BalanceChange>,
    pub confirmed_local_execution: Option<bool>,
}

impl TryFrom<SuiTransactionBlockResponse> for FastPathTransactionBlockResponse {
    type Error = anyhow::Error;

    fn try_from(response: SuiTransactionBlockResponse) -> Result<Self, Self::Error> {
        let SuiTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            object_changes,
            balance_changes,
            timestamp_ms: _,
            confirmed_local_execution,
            checkpoint: _,
            errors,
        } = response;

        let transaction = transaction.ok_or_else(|| {
            anyhow::anyhow!(
                "Transaction is None in FastPathTransactionBlockResponse of digest {:?}.",
                digest
            )
        })?;
        let effects = effects.ok_or_else(|| {
            anyhow::anyhow!(
                "Effects is None in FastPathTransactionBlockResponse of digest {:?}.",
                digest
            )
        })?;
        let events = events.ok_or_else(|| {
            anyhow::anyhow!(
                "Events is None in FastPathTransactionBlockResponse of digest {:?}.",
                digest
            )
        })?;
        let object_changes = object_changes.ok_or_else(|| {
            anyhow::anyhow!(
                "ObjectChanges is None in FastPathTransactionBlockResponse of digest {:?}.",
                digest
            )
        })?;
        let balance_changes = balance_changes.ok_or_else(|| {
            anyhow::anyhow!(
                "BalanceChanges is None in FastPathTransactionBlockResponse of digest {:?}.",
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

        Ok(FastPathTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            object_changes,
            balance_changes,
            confirmed_local_execution,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointTransactionBlockResponse {
    pub digest: TransactionDigest,
    /// Transaction input data
    pub transaction: SuiTransactionBlock,
    pub raw_transaction: Vec<u8>,
    pub effects: SuiTransactionBlockEffects,
    pub events: SuiTransactionBlockEvents,
    pub timestamp_ms: u64,
    pub confirmed_local_execution: Option<bool>,
    pub checkpoint: CheckpointSequenceNumber,
}

impl TryFrom<SuiTransactionBlockResponse> for CheckpointTransactionBlockResponse {
    type Error = anyhow::Error;

    fn try_from(response: SuiTransactionBlockResponse) -> Result<Self, Self::Error> {
        let SuiTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            object_changes: _,
            balance_changes: _,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
            errors,
        } = response;

        let transaction = transaction.ok_or_else(|| {
            anyhow::anyhow!(
                "Transaction is None in SuiTransactionBlockFullResponse of digest {:?}.",
                digest
            )
        })?;
        let effects = effects.ok_or_else(|| {
            anyhow::anyhow!(
                "Effects is None in SuiTransactionBlockFullResponse of digest {:?}.",
                digest
            )
        })?;
        let events = events.ok_or_else(|| {
            anyhow::anyhow!(
                "Events is None in SuiTransactionBlockFullResponse of digest {:?}.",
                digest
            )
        })?;
        let timestamp_ms = timestamp_ms.ok_or_else(|| {
            anyhow::anyhow!(
                "TimestampMs is None in SuiTransactionBlockFullResponse of digest {:?}.",
                digest
            )
        })?;
        let checkpoint = checkpoint.ok_or_else(|| {
            anyhow::anyhow!(
                "Checkpoint is None in SuiTransactionBlockFullResponse of digest {:?}.",
                digest
            )
        })?;
        if raw_transaction.is_empty() {
            return Err(anyhow::anyhow!(
                "Unexpected empty RawTransaction in SuiTransactionBlockFullResponse of digest {:?}.",
                digest
            ));
        }
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Errors in SuiTransactionBlockFullResponse of digest {:?}: {:?}",
                digest,
                errors
            ));
        }

        Ok(CheckpointTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
        })
    }
}

impl CheckpointTransactionBlockResponse {
    pub fn get_input_objects(&self, epoch: u64) -> Result<Vec<InputObject>, IndexerError> {
        let raw_tx = self.raw_transaction.clone();
        let sender_signed_data: SenderSignedData = bcs::from_bytes(&raw_tx).map_err(|err| {
            IndexerError::SerdeError(format!(
                "Failed converting transaction {:?} from bytes {:?} to SenderSignedData with error: {:?}",
                self.digest.clone(), raw_tx, err
            ))
        })?;
        let input_objects: Vec<InputObject> =
            sender_signed_data
                .transaction_data()
                .input_objects()
                .map_err(|err| {
                    IndexerError::InvalidArgumentError(format!(
                    "Failed getting input objects of transaction {:?} from {:?} with error: {:?}",
                    self.digest.clone(), raw_tx, err
                ))
                })?
                .into_iter()
                .map(|obj_kind| InputObject {
                    id: None,
                    transaction_digest: self.digest.to_string(),
                    checkpoint_sequence_number: self.checkpoint as i64,
                    epoch: epoch as i64,
                    object_id: obj_kind.object_id().to_string(),
                    object_version: obj_kind.version().map(|v| v.value() as i64),
                })
                .collect();
        Ok(input_objects)
    }

    pub fn get_move_calls(&self, epoch: u64, checkpoint: u64) -> Vec<MoveCall> {
        let tx_kind = self.transaction.data.transaction();
        let sender = self.transaction.data.sender();
        match tx_kind {
            SuiTransactionBlockKind::ProgrammableTransaction(pt) => {
                let move_calls: Vec<MoveCall> = pt
                    .commands
                    .clone()
                    .into_iter()
                    .filter_map(move |command| match command {
                        SuiCommand::MoveCall(m) => Some(MoveCall {
                            id: None,
                            transaction_digest: self.digest.to_string(),
                            checkpoint_sequence_number: checkpoint as i64,
                            epoch: epoch as i64,
                            sender: sender.to_string(),
                            move_package: m.package.to_string(),
                            move_module: m.module,
                            move_function: m.function,
                        }),
                        _ => None,
                    })
                    .collect();
                Some(move_calls)
            }
            _ => None,
        }
        .unwrap_or_default()
    }

    pub fn get_recipients(&self, epoch: u64, checkpoint: u64) -> Vec<Recipient> {
        let created = self.effects.created().iter();
        let mutated = self.effects.mutated().iter();
        let unwrapped = self.effects.unwrapped().iter();
        created
            .chain(mutated)
            .chain(unwrapped)
            .filter_map(|obj_ref| match obj_ref.owner {
                Owner::AddressOwner(address) => Some(Recipient {
                    id: None,
                    transaction_digest: self.digest.to_string(),
                    checkpoint_sequence_number: checkpoint as i64,
                    epoch: epoch as i64,
                    sender: self.transaction.data.sender().to_string(),
                    recipient: address.to_string(),
                }),
                _ => None,
            })
            .collect()
    }

    pub fn get_addresses(&self, epoch: u64, checkpoint: u64) -> Vec<Address> {
        let mut addresses = self
            .get_recipients(epoch, checkpoint)
            .into_iter()
            .map(|r| r.recipient)
            .collect::<Vec<String>>();
        addresses.push(self.transaction.data.sender().to_string());
        addresses
            .into_iter()
            .map(|r| Address {
                account_address: r,
                first_appearance_tx: self.digest.to_string(),
                first_appearance_time: self.timestamp_ms as i64,
            })
            .collect::<Vec<Address>>()
    }
}

pub struct TemporaryTransactionBlockResponseStore {
    pub digest: TransactionDigest,
    /// Transaction input data
    pub transaction: SuiTransactionBlock,
    pub raw_transaction: Vec<u8>,
    pub effects: SuiTransactionBlockEffects,
    pub events: SuiTransactionBlockEvents,
    pub object_changes: Option<Vec<ObjectChange>>,
    pub balance_changes: Option<Vec<BalanceChange>>,
    pub timestamp_ms: Option<u64>,
    pub confirmed_local_execution: Option<bool>,
    pub checkpoint: Option<CheckpointSequenceNumber>,
}

impl From<FastPathTransactionBlockResponse> for TemporaryTransactionBlockResponseStore {
    fn from(value: FastPathTransactionBlockResponse) -> Self {
        let FastPathTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            object_changes,
            balance_changes,
            confirmed_local_execution,
        } = value;

        TemporaryTransactionBlockResponseStore {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            object_changes: Some(object_changes),
            balance_changes: Some(balance_changes),
            timestamp_ms: None,
            confirmed_local_execution,
            checkpoint: None,
        }
    }
}

impl From<CheckpointTransactionBlockResponse> for TemporaryTransactionBlockResponseStore {
    fn from(value: CheckpointTransactionBlockResponse) -> Self {
        let CheckpointTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
        } = value;

        TemporaryTransactionBlockResponseStore {
            digest,
            transaction,
            raw_transaction,
            effects,
            events,
            object_changes: None,
            balance_changes: None,
            timestamp_ms: Some(timestamp_ms),
            confirmed_local_execution,
            checkpoint: Some(checkpoint),
        }
    }
}

// SuiTransactionBlockResponseWithOptions is only used on the reading path
pub struct SuiTransactionBlockResponseWithOptions {
    pub response: SuiTransactionBlockResponse,
    pub options: SuiTransactionBlockResponseOptions,
}

impl From<SuiTransactionBlockResponseWithOptions> for SuiTransactionBlockResponse {
    fn from(value: SuiTransactionBlockResponseWithOptions) -> Self {
        let SuiTransactionBlockResponseWithOptions { response, options } = value;

        SuiTransactionBlockResponse {
            digest: response.digest,
            transaction: options.show_input.then_some(response.transaction).flatten(),
            raw_transaction: options
                .show_raw_input
                .then_some(response.raw_transaction)
                .unwrap_or_default(),
            effects: options.show_effects.then_some(response.effects).flatten(),
            events: options.show_events.then_some(response.events).flatten(),
            object_changes: options
                .show_object_changes
                .then_some(response.object_changes)
                .flatten(),
            balance_changes: options
                .show_balance_changes
                .then_some(response.balance_changes)
                .flatten(),
            timestamp_ms: response.timestamp_ms,
            confirmed_local_execution: response.confirmed_local_execution,
            checkpoint: response.checkpoint,
            errors: vec![],
        }
    }
}
