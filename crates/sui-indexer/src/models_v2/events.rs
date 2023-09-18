// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::prelude::*;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::identifier::Identifier;
use move_core_types::value::MoveStruct;

use sui_json_rpc_types::{SuiEvent, SuiMoveStruct};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::event::EventID;
use sui_types::object::{MoveObject, ObjectFormatOptions};
use sui_types::parse_sui_struct_tag;

use crate::errors::IndexerError;
use crate::schema_v2::events;
use crate::types_v2::IndexedEvent;

#[derive(Queryable, Insertable, Debug, Clone)]
#[diesel(table_name = events)]
pub struct StoredEvent {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub checkpoint_sequence_number: i64,
    pub senders: Vec<Vec<u8>>,
    pub package: Vec<u8>,
    pub module: String,
    pub event_type: String,
    pub bcs: Vec<u8>,
    pub timestamp_ms: i64,
}

impl From<IndexedEvent> for StoredEvent {
    fn from(event: IndexedEvent) -> Self {
        Self {
            tx_sequence_number: event.tx_sequence_number as i64,
            event_sequence_number: event.event_sequence_number as i64,
            transaction_digest: event.transaction_digest.into_inner().to_vec(),
            checkpoint_sequence_number: event.checkpoint_sequence_number as i64,
            senders: event
                .senders
                .into_iter()
                .map(|sender| sender.to_vec())
                .collect(),
            package: event.package.to_vec(),
            module: event.module.clone(),
            event_type: event.event_type.clone(),
            bcs: event.bcs.clone(),
            timestamp_ms: event.timestamp_ms as i64,
        }
    }
}

impl StoredEvent {
    pub fn try_into_sui_event(
        self,
        module_cache: &impl GetModule,
    ) -> Result<SuiEvent, IndexerError> {
        let package_id = ObjectID::from_bytes(self.package.clone()).map_err(|_e| {
            IndexerError::PersistentStorageDataCorruptionError(format!(
                "Failed to parse event package ID: {:?}",
                self.package
            ))
        })?;
        // Note: SuiEvent only has one sender today, so we always use the first one.
        let sender = self.senders.first().ok_or_else(|| {
            IndexerError::PersistentStorageDataCorruptionError(
                "Event senders should contain at least one address".to_string(),
            )
        })?;
        let sender = SuiAddress::from_bytes(sender).map_err(|_e| {
            IndexerError::PersistentStorageDataCorruptionError(format!(
                "Failed to parse event sender address: {:?}",
                sender
            ))
        })?;

        let type_ = parse_sui_struct_tag(&self.event_type)?;

        let layout = MoveObject::get_layout_from_struct_tag(
            type_.clone(),
            ObjectFormatOptions::default(),
            module_cache,
        )?;
        let move_object = MoveStruct::simple_deserialize(&self.bcs, &layout)
            .map_err(|e| IndexerError::SerdeError(e.to_string()))?;
        let parsed_json = SuiMoveStruct::from(move_object).to_json_value();
        let tx_digest = bcs::from_bytes(&self.transaction_digest).map_err(|e| {
            IndexerError::SerdeError(format!(
                "Failed to parse transaction digest: {:?}, error: {}",
                self.transaction_digest, e
            ))
        })?;
        Ok(SuiEvent {
            id: EventID {
                tx_digest,
                event_seq: self.event_sequence_number as u64,
            },
            package_id,
            transaction_module: Identifier::from_str(&self.module)?,
            sender,
            type_,
            bcs: self.bcs,
            parsed_json,
            timestamp_ms: Some(self.timestamp_ms as u64),
        })
    }
}
