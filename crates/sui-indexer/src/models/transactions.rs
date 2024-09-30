// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use diesel::prelude::*;

use move_core_types::annotated_value::{MoveDatatypeLayout, MoveTypeLayout};
use move_core_types::language_storage::TypeTag;
use sui_json_rpc_types::{SuiEvent, SuiTransactionBlockEffects, SuiTransactionBlockEvents};
use sui_package_resolver::{PackageStore, Resolver};
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::event::Event;
use sui_types::transaction::SenderSignedData;

use crate::errors::IndexerError;
use crate::schema::transactions;
use crate::types::IndexedTransaction;
use crate::types::IndexerResult;

#[derive(Clone, Debug, Queryable, Insertable, QueryableByName, Selectable)]
#[diesel(table_name = transactions)]
pub struct StoredTransaction {
    pub tx_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub checkpoint_sequence_number: i64,
    pub timestamp_ms: i64,
    pub object_changes: Vec<Option<Vec<u8>>>,
    pub balance_changes: Vec<Option<Vec<u8>>>,
    pub events: Vec<Option<Vec<u8>>>,
    pub transaction_kind: i16,
    pub success_command_count: i16,
}

pub type StoredTransactionEvents = Vec<Option<Vec<u8>>>;

#[derive(Debug, Queryable)]
pub struct TxSeq {
    pub seq: i64,
}

impl Default for TxSeq {
    fn default() -> Self {
        Self { seq: -1 }
    }
}

#[derive(Clone, Debug, Queryable)]
pub struct StoredTransactionTimestamp {
    pub tx_sequence_number: i64,
    pub timestamp_ms: i64,
}

#[derive(Clone, Debug, Queryable)]
pub struct StoredTransactionCheckpoint {
    pub tx_sequence_number: i64,
    pub checkpoint_sequence_number: i64,
}

#[derive(Clone, Debug, Queryable)]
pub struct StoredTransactionSuccessCommandCount {
    pub tx_sequence_number: i64,
    pub checkpoint_sequence_number: i64,
    pub success_command_count: i16,
    pub timestamp_ms: i64,
}

impl From<&IndexedTransaction> for StoredTransaction {
    fn from(tx: &IndexedTransaction) -> Self {
        StoredTransaction {
            tx_sequence_number: tx.tx_sequence_number as i64,
            transaction_digest: tx.tx_digest.into_inner().to_vec(),
            raw_transaction: bcs::to_bytes(&tx.sender_signed_data).unwrap(),
            raw_effects: bcs::to_bytes(&tx.effects).unwrap(),
            checkpoint_sequence_number: tx.checkpoint_sequence_number as i64,
            object_changes: tx
                .object_changes
                .iter()
                .map(|oc| Some(bcs::to_bytes(&oc).unwrap()))
                .collect(),
            balance_changes: tx
                .balance_change
                .iter()
                .map(|bc| Some(bcs::to_bytes(&bc).unwrap()))
                .collect(),
            events: tx
                .events
                .iter()
                .map(|e| Some(bcs::to_bytes(&e).unwrap()))
                .collect(),
            timestamp_ms: tx.timestamp_ms as i64,
            transaction_kind: tx.transaction_kind.clone() as i16,
            success_command_count: tx.successful_tx_num as i16,
        }
    }
}

impl StoredTransaction {
    pub fn get_balance_len(&self) -> usize {
        self.balance_changes.len()
    }

    pub fn get_balance_at_idx(&self, idx: usize) -> Option<Vec<u8>> {
        self.balance_changes.get(idx).cloned().flatten()
    }

    pub fn get_object_len(&self) -> usize {
        self.object_changes.len()
    }

    pub fn get_object_at_idx(&self, idx: usize) -> Option<Vec<u8>> {
        self.object_changes.get(idx).cloned().flatten()
    }

    pub fn get_event_len(&self) -> usize {
        self.events.len()
    }

    pub fn get_event_at_idx(&self, idx: usize) -> Option<Vec<u8>> {
        self.events.get(idx).cloned().flatten()
    }

    pub(crate) fn try_into_sender_signed_data(&self) -> IndexerResult<SenderSignedData> {
        let sender_signed_data: SenderSignedData =
            bcs::from_bytes(&self.raw_transaction).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Can't convert raw_transaction of {} into SenderSignedData. Error: {e}",
                    self.tx_sequence_number
                ))
            })?;
        Ok(sender_signed_data)
    }

    pub fn try_into_sui_transaction_effects(&self) -> IndexerResult<SuiTransactionBlockEffects> {
        let effects: TransactionEffects = bcs::from_bytes(&self.raw_effects).map_err(|e| {
            IndexerError::PersistentStorageDataCorruptionError(format!(
                "Can't convert raw_effects of {} into TransactionEffects. Error: {e}",
                self.tx_sequence_number
            ))
        })?;
        let effects = SuiTransactionBlockEffects::try_from(effects)?;
        Ok(effects)
    }
}

pub fn stored_events_to_events(
    stored_events: StoredTransactionEvents,
) -> Result<Vec<Event>, IndexerError> {
    stored_events
        .into_iter()
        .map(|event| match event {
            Some(event) => {
                let event: Event = bcs::from_bytes(&event).map_err(|e| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "Can't convert event bytes into Event. Error: {e}",
                    ))
                })?;
                Ok(event)
            }
            None => Err(IndexerError::PersistentStorageDataCorruptionError(
                "Event should not be null".to_string(),
            )),
        })
        .collect::<Result<Vec<Event>, IndexerError>>()
}

pub async fn tx_events_to_sui_tx_events(
    tx_events: TransactionEvents,
    package_resolver: Arc<Resolver<impl PackageStore>>,
    tx_digest: TransactionDigest,
    timestamp: u64,
) -> Result<Option<SuiTransactionBlockEvents>, IndexerError> {
    let mut sui_event_futures = vec![];
    let tx_events_data_len = tx_events.data.len();
    for tx_event in tx_events.data.clone() {
        let package_resolver_clone = package_resolver.clone();
        sui_event_futures.push(tokio::task::spawn(async move {
            let resolver = package_resolver_clone;
            resolver
                .type_layout(TypeTag::Struct(Box::new(tx_event.type_.clone())))
                .await
        }));
    }
    let event_move_type_layouts = futures::future::join_all(sui_event_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            IndexerError::ResolveMoveStructError(format!(
                "Failed to convert to sui event with Error: {e}",
            ))
        })?;
    let event_move_datatype_layouts = event_move_type_layouts
        .into_iter()
        .filter_map(|move_type_layout| match move_type_layout {
            MoveTypeLayout::Struct(s) => Some(MoveDatatypeLayout::Struct(s)),
            MoveTypeLayout::Enum(e) => Some(MoveDatatypeLayout::Enum(e)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(tx_events_data_len == event_move_datatype_layouts.len());
    let sui_events = tx_events
        .data
        .into_iter()
        .enumerate()
        .zip(event_move_datatype_layouts)
        .map(|((seq, tx_event), move_datatype_layout)| {
            SuiEvent::try_from(
                tx_event,
                tx_digest,
                seq as u64,
                Some(timestamp),
                move_datatype_layout,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sui_tx_events = SuiTransactionBlockEvents { data: sui_events };
    Ok(Some(sui_tx_events))
}
